use std::future::Future;
use std::option::Option::Some;

use async_std::net::{TcpListener, TcpStream, ToSocketAddrs};
use async_std::sync::Arc;
use async_std::task;
use async_std::task::JoinHandle;
use futures::{AsyncBufReadExt, AsyncWriteExt, StreamExt};
use futures::channel::mpsc::UnboundedReceiver;
use futures::io::BufReader;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn spawn_and_log_error(fut: impl Future<Output=Result<()>> + Send + 'static) -> JoinHandle<()> {
    task::spawn(async {
        if let Err(e) = fut.await {
            eprintln!("{}", e);
        }
    })
}

async fn connection_loop(stream: Arc<TcpStream>) -> Result<()> {
    let reader = BufReader::new(&*stream);
    let mut lines = reader.lines();
    let name = match lines.next().await {
        None => { Err("peer disconnected immediately")? }
        Some(line) => { line? }
    };
    println!("New user connected. name: {}", name);
    while let Some(line) = lines.next().await {
        let line = line?;
        let (dest, msg) = match line.find(":") {
            None => { continue; }
            Some(idx) => { (&line[0..idx], line[idx + 1..].trim()) }
        };
        let _dest: Vec<String> = dest.split(",").map(|dest| dest.trim().to_string()).collect();
        let _msg = msg.to_string();
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
    while let Some(stream) = incoming.next().await {
        let stream = Arc::new(stream?);
        let handle = spawn_and_log_error(connection_loop(stream.clone()));
        handle.await;
    }
    Ok(())
}

fn main() -> Result<()> {
    task::block_on(accept_loop("127.0.0.1:9527"))
}
