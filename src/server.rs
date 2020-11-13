use std::collections::HashMap;
use std::option::Option::Some;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_std::io::BufReader;
use async_std::net::{Shutdown, TcpListener, ToSocketAddrs};
use bimap::{BiHashMap, Overwritten};
use futures::{AsyncBufReadExt, AsyncWriteExt, SinkExt, StreamExt};
use futures::channel::mpsc;

use crate::core::*;

enum Event {
    NewPeer {
        id: usize,
        name: String,
        stream: Arc<TcpStream>,
    },
    Disconnected {
        id: usize,
    },
    Message {
        from_id: usize,
        to: Vec<String>,
        msg: String,
    },
}

async fn broker_loop(mut events: Receiver<Event>) {
    let mut id_name_pairs = BiHashMap::<usize, String>::new();
    let mut peers = HashMap::<usize, Sender<String>>::new();
    let mut writer_handles = HashMap::<usize, JoinHandle<()>>::new();
    while let Some(event) = events.next().await {
        match event {
            Event::NewPeer { id, name, stream, } => {
                println!("New user connected. name: {}", name);
                match id_name_pairs.insert(id, name) {
                    Overwritten::Neither => {}
                    Overwritten::Right(id, name) => {
                        peers.remove(&id);
                        writer_handles.remove(&id).unwrap().await;
                        println!("{} disconnected by duplicate.", name);
                    }
                    _ => { unreachable!(); }
                }

                let (message_sender, message_receiver) = mpsc::unbounded();
                peers.insert(id, message_sender);
                let writer_handle = spawn_and_log_error(connection_writer_loop(message_receiver, stream));
                writer_handles.insert(id, writer_handle);
            }
            Event::Disconnected { id } => {
                if let Some((id, name)) = id_name_pairs.remove_by_left(&id) {
                    peers.remove(&id).unwrap();
                    writer_handles.remove(&id).unwrap().await;
                    println!("{} disconnected.", name);
                }
            }
            Event::Message { from_id, to, msg } => {
                let from = id_name_pairs.get_by_left(&from_id).unwrap();
                let msg = format!("from {}: {}\n", from, msg);
                for to_id in to.into_iter().filter_map(|to_name| id_name_pairs.get_by_right(&to_name)) {
                    await_and_log_error(peers.get_mut(to_id).unwrap().send(msg.clone())).await
                }
            }
        };
    }

    drop(peers);
    for (_name, writer_handle) in writer_handles {
        writer_handle.await;
    }
}

async fn connection_loop(stream: TcpStream, mut events_sender: Sender<Event>) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();
    let name = match lines.next().await {
        None => Err("peer disconnected immediately")?,
        Some(line) => line?,
    };

    static ID: AtomicUsize = AtomicUsize::new(0);
    let id = ID.fetch_add(1, Ordering::Relaxed);

    events_sender.send(Event::NewPeer { id, name: name.clone(), stream: stream.clone() }).await.unwrap();

    while let Some(line) = lines.next().await {
        let line = line?;
        let (dest, msg) = match line.find(":") {
            None => continue,
            Some(idx) => (&line[0..idx], line[idx + 1..].trim()),
        };
        let to: Vec<String> = dest
            .split(",")
            .map(|dest| dest.trim().to_string())
            .collect();
        let msg = msg.to_string();
        events_sender.send(Event::Message { from_id: id, to, msg }).await.unwrap();
    }

    events_sender.send(Event::Disconnected { id }).await.unwrap();
    Ok(())
}

async fn connection_writer_loop(mut message_receiver: Receiver<String>, stream: Arc<TcpStream>) -> Result<()> {
    while let Some(message) = message_receiver.next().await {
        let mut stream = &*stream;
        stream.write_all(message.as_bytes()).await?;
    }
    stream.shutdown(Shutdown::Both)?;
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

pub(crate) async fn run_server() -> Result<()> {
    accept_loop("127.0.0.1:9527").await?;
    Ok(())
}