use std::env;

use crate::core::*;

mod core;
mod server;
mod client;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.contains(&"-c".to_string()) {
        task::block_on(client::run_client())
    } else {
        task::block_on(server::run_server())
    }
}
