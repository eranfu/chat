use std::collections::HashMap;
use std::future::Future;
use std::option::Option::Some;

use async_std::net::{TcpListener, TcpStream, ToSocketAddrs};
use async_std::sync::Arc;
use async_std::task;
use async_std::task::JoinHandle;
use futures::{AsyncBufReadExt, AsyncWriteExt, SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::channel::mpsc;
use futures::io::BufReader;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

enum Event {
    NewPeer {
        name: String,
        stream: Arc<TcpStream>,
    },
    Message {
        from: String,
        to: Vec<String>,
        msg: String,
    },
}

async fn await_and_log_error(fut: impl Future<Output=std::result::Result<(), impl Into<Box<dyn std::error::Error + Send + Sync>>>>) -> () {
    if let Err(e) = fut.await {
        eprintln!("{}", e.into());
    }
}

async fn spawn_and_log_error(fut: impl Future<Output=Result<()>> + Send + 'static) -> JoinHandle<()> {
    task::spawn(async { await_and_log_error(fut).await; })
}

async fn broker_loop(mut events: UnboundedReceiver<Event>) {
    let mut peers = HashMap::new();
    let mut writer_handles = Vec::new();
    while let Some(event) = events.next().await {
        match event {
            Event::NewPeer { name, stream } => {
                println!("New user connected. name: {}", name);
                let (sender, receiver) = mpsc::unbounded();
                peers.insert(name, sender);
                writer_handles.push(spawn_and_log_error(connection_writer_loop(receiver, stream)));
            }
            Event::Message { from, to, msg } => {
                let msg = format!("from {}: {}", from, msg);
                for to in to {
                    if let Some(sender) = peers.get_mut(&to) {
                        await_and_log_error(sender.send(msg.clone())).await;
                    }
                }
            }
        }
    }
    drop(peers);
    for writer_handle in writer_handles {
        writer_handle.await;
    }
}

async fn connection_loop(stream: TcpStream, mut events_sender: UnboundedSender<Event>) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();
    let name = match lines.next().await {
        None => { Err("peer disconnected immediately")? }
        Some(line) => { line? }
    };
    events_sender.send(Event::NewPeer { name: name.clone(), stream: stream.clone() }).await.unwrap();

    while let Some(line) = lines.next().await {
        let line = line?;
        let (dest, msg) = match line.find(":") {
            None => { continue; }
            Some(idx) => { (&line[0..idx], line[idx + 1..].trim()) }
        };
        let to: Vec<String> = dest.split(",").map(|dest| dest.trim().to_string()).collect();
        let msg = msg.to_string();
        events_sender.send(Event::Message { from: name.clone(), to, msg }).await.unwrap();
    }
    Ok(())
}

async fn connection_writer_loop(mut messages: UnboundedReceiver<String>, stream: Arc<TcpStream>) -> Result<()> {
    while let Some(message) = messages.next().await {
        let mut stream = &*stream;
        stream.write_all(message.as_bytes()).await?
    }
    Ok(())
}

async fn accept_loop(addr: impl ToSocketAddrs) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let mut incoming = listener.incoming();
    let (events_sender, events_receiver) = mpsc::unbounded();
    let broker_loop_handle = task::spawn(broker_loop(events_receiver));
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        println!("Accepting from: {}", stream.peer_addr()?);
        let _connection_loop = spawn_and_log_error(connection_loop(stream, events_sender.clone()));
    }
    drop(events_sender);
    broker_loop_handle.await;
    Ok(())
}

fn main() -> Result<()> {
    task::block_on(accept_loop("127.0.0.1:9527"))
}
