use std::collections::HashMap;

use async_std::io::BufReader;
use async_std::net::{TcpListener, ToSocketAddrs};
use futures::{AsyncBufReadExt, AsyncWriteExt, SinkExt, StreamExt};
use futures::channel::mpsc;
use futures::select;

use crate::core::*;

enum Disconnect {}

enum Event {
    NewPeer {
        name: String,
        stream: Arc<TcpStream>,
        disconnect_receiver: Receiver<Disconnect>,
    },
    Message {
        from: String,
        to: Vec<String>,
        msg: String,
    },
}

async fn broker_loop(events: Receiver<Event>) {
    let mut peers = HashMap::<String, Sender<String>>::new();
    let mut writer_handles = HashMap::<String, JoinHandle<()>>::new();
    let (writer_shutdown_sender, writer_shutdown_receiver) = mpsc::unbounded::<String>();
    let mut writer_shutdown_receiver = writer_shutdown_receiver.fuse();
    let mut events = events.fuse();
    loop {
        let event = select! {
            writer_shutdown = writer_shutdown_receiver.next() => match writer_shutdown {
                None => { unreachable!(); },
                Some(writer_shutdown) => {
                    let name = writer_shutdown;
                    assert!(peers.remove(&name).is_some());
                    continue;
                },
            },
            event = events.next() => match event {
                None => { break; },
                Some(event) => { event },
            },
        };
        match event {
            Event::NewPeer {
                name,
                stream,
                disconnect_receiver,
            } => {
                println!("New user connected. name: {}", name);
                if let Some(handle) = writer_handles.remove(&name) {
                    handle.cancel().await;
                };

                let (message_sender, mut message_receiver) = mpsc::unbounded();
                {
                    let mut writer_shutdown_sender = writer_shutdown_sender.clone();
                    let name = name.clone();
                    writer_handles.insert(
                        name.clone(),
                        spawn_and_log_error(async move {
                            connection_writer_loop(disconnect_receiver, &mut message_receiver, stream).await?;
                            writer_shutdown_sender.send(name).await?;
                            Ok(())
                        }),
                    );
                }
                peers.insert(name.clone(), message_sender);
            }
            Event::Message { from, to, msg } => {
                let msg = format!("from {}: {}\n", from, msg);
                for to in to {
                    if let Some(sender) = peers.get_mut(&to) {
                        await_and_log_error(sender.send(msg.clone())).await;
                    }
                }
            }
        };
    }

    drop(peers);
    drop(writer_shutdown_sender);
    drop(writer_handles);
    while let Some(_) = writer_shutdown_receiver.next().await {}
}

async fn connection_loop(
    stream: TcpStream,
    mut events_sender: Sender<Event>,
) -> Result<()> {
    let stream = Arc::new(stream);
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();
    let name = match lines.next().await {
        None => Err("peer disconnected immediately")?,
        Some(line) => line?,
    };
    let (_disconnect_sender, disconnect_receiver) = mpsc::unbounded();
    events_sender
        .send(Event::NewPeer {
            name: name.clone(),
            stream: stream.clone(),
            disconnect_receiver,
        })
        .await
        .unwrap();

    while let Some(line) = lines.next().await {
        let line = line?;
        let (dest, msg) = match line.find(":") {
            None => {
                continue;
            }
            Some(idx) => (&line[0..idx], line[idx + 1..].trim()),
        };
        let to: Vec<String> = dest
            .split(",")
            .map(|dest| dest.trim().to_string())
            .collect();
        let msg = msg.to_string();
        events_sender
            .send(Event::Message {
                from: name.clone(),
                to,
                msg,
            })
            .await
            .unwrap();
    }
    Ok(())
}

async fn connection_writer_loop(
    disconnect_receiver: Receiver<Disconnect>,
    message_receiver: &mut Receiver<String>,
    stream: Arc<TcpStream>,
) -> Result<()> {
    let mut messages = message_receiver.fuse();
    let mut disconnect_receiver = disconnect_receiver.fuse();
    loop {
        select! {
            message = messages.next() => match message {
                None => { break; },
                Some(message) => {
                    let mut stream = &*stream;
                    stream.write_all(message.as_bytes()).await?;
                },
            },
            disconnect = disconnect_receiver.next() => match disconnect {
                None => { break; },
                Some(_) => { unreachable!(); }
            }
        }
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

pub(crate) async fn run_server() -> Result<()> {
    accept_loop("127.0.0.1:9527").await?;
    Ok(())
}