use std::future::Future;

use async_std::net::{TcpListener, TcpStream, ToSocketAddrs};
use async_std::task;
use async_std::task::JoinHandle;
use futures::{AsyncBufReadExt, StreamExt};
use futures::io::BufReader;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

async fn spawn_and_log_error(fut: impl Future<Output=Result<()>> + Send + 'static) -> JoinHandle<()> {
    task::spawn(async {
        if let Err(e) = fut.await {
            eprintln!("{}", e);
        }
    })
}

async fn connection_loop(stream: TcpStream) -> Result<()> {
    let reader = BufReader::new(stream);
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

async fn accept_loop(addr: impl ToSocketAddrs) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        let stream = stream?;
        let handle = spawn_and_log_error(connection_loop(stream));
        handle.await;
    }
    Ok(())
}

fn main() -> Result<()> {
    task::block_on(accept_loop("127.0.0.1:9527"))
}
