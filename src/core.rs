pub(crate) use std::future::Future;

pub(crate) use async_std::net::TcpStream;
pub(crate) use async_std::sync::Arc;
pub(crate) use async_std::task;
pub(crate) use async_std::task::JoinHandle;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub(crate) type Receiver<T> = UnboundedReceiver<T>;
pub(crate) type Sender<T> = UnboundedSender<T>;


pub(crate) async fn await_and_log_error(fut: impl Future<Output=std::result::Result<(), impl Into<Box<dyn std::error::Error + Send + Sync>>>>) -> () {
    if let Err(e) = fut.await {
        eprintln!("{}", e.into());
    }
}

pub(crate) fn spawn_and_log_error(fut: impl Future<Output=Result<()>> + Send + 'static) -> JoinHandle<()> {
    task::spawn(async {
        await_and_log_error(fut).await;
    })
}