mod server;
mod http_parser;
mod utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>>  {
    let (manager, tx) = server::Manager::create();
    let proxy = server::ProxyServer::new(3000, "0.0.0.0", tx);
    proxy.listen().await?;
    Ok(())
}
