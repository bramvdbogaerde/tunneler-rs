mod client;
mod http_parser;
mod server;
mod utils;
mod protocol;

use crate::utils::error;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = env::args()
        .nth(1)
        .ok_or_else(|| error("invalid usage, command required"))?;

    match command.as_str() {
        "server" => {
            let (manager, tx) = server::Manager::create();
            let proxy = server::ProxyServer::new(3000, "0.0.0.0", tx);
            //proxy.listen().await?;
            manager.listen().await?;
        }
        "client" => {
            let client = client::Client::new("127.0.0.1:5555");
            client.run().await?;
        }
        _ => return Err(error("invalid command").into())
    };

    Ok(())
}
