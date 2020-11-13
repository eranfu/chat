use async_std::io::{BufReader, stdin};
use async_std::net::ToSocketAddrs;
use futures::{AsyncBufReadExt, AsyncWriteExt, select, StreamExt};

use crate::core::*;

async fn client_loop(addr: impl ToSocketAddrs) -> Result<()> {
    let connection = TcpStream::connect(addr).await?;
    let (connection_reader, mut connection_writer) = (&connection, &connection);
    let mut lines_from_server = BufReader::new(connection_reader).lines().fuse();
    let mut lines_from_stdin = BufReader::new(stdin()).lines().fuse();

    loop {
        select! {
            line_from_server = lines_from_server.next() => match line_from_server {
                None => { break; },
                Some(line_from_server) => { println!("{}", line_from_server?); },
            },
            line_from_stdin = lines_from_stdin.next() => match line_from_stdin {
                None => { break; },
                Some(line_from_stdin) => {
                    connection_writer.write_all(line_from_stdin?.as_bytes()).await?;
                    connection_writer.write_all(b"\n").await?;
                },
            }
        }
    }

    Ok(())
}

pub(crate) async fn run_client() -> Result<()> {
    client_loop("127.0.0.1:9527").await
}
