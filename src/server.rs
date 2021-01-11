use crate::{http_parser::HttpRequest, utils::error};
use std::collections::HashMap;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{
    mpsc,
    mpsc::{Receiver, Sender},
};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct ProxyServer {
    port: usize,
    host: String,
    request_sender: Sender<Request>,
}

impl ProxyServer {
    pub fn new<S: Into<String>>(
        port: usize,
        host: S,
        request_sender: Sender<Request>,
    ) -> ProxyServer {
        ProxyServer {
            port,
            host: host.into(),
            request_sender,
        }
    }

    async fn process(manager: Sender<Request>, mut socket: TcpStream) -> std::io::Result<()> {
        let request = HttpRequest::parse(&mut socket).await?;
        let host = match request.headers.get("Host") {
            Some(h) => h,
            // we simply ignore the request if it does not have a valid host header
            None => return Ok(()),
        };

        println!("Host header: {}", host);

        let (tx, mut rx) = mpsc::channel(1);
        let request = Request {
            id: host.clone(),
            contents: request.bytes,
            reply_to: tx,
        };

        manager
            .send(request)
            .await
            .map_err(|_| error("could not send request"))?;

        let response = rx.recv()
            .await
            .ok_or(error("could not receive request"))?;

        socket.write_all(&response.contents).await?;

        Ok(())
    }

    pub async fn listen(&self) -> std::io::Result<()> {
        let listener = TcpListener::bind(format!("{}:{}", self.host, self.port)).await?;
        loop {
            let (socket, _) = listener.accept().await?;
            let manager = self.request_sender.clone();
            tokio::spawn(async move {
                Self::process(manager, socket)
                    .await
                    .expect("an error occurred while processing the request");
            });
        }
    }
}

#[derive(Debug, Clone)]
struct Connection;

#[derive(Debug, Clone)]
pub struct Request {
    id: String,
    contents: Vec<u8>,
    reply_to: Sender<Response>,
}

pub struct Response {
    contents: Vec<u8>
}

/// Structure holds all the connection, and mappings between IDs and connections
pub struct Manager {
    /// A mapping from automatically generated names to connections
    connections: HashMap<String, Connection>,
    /// A channel on which requests will arrive
    requests_receiver: Receiver<Request>,
}

impl Manager {
    pub fn new(requests_receiver: Receiver<Request>) -> Manager {
        Manager {
            connections: HashMap::new(),
            requests_receiver,
        }
    }

    /// Same as new but creates an mpsc channel itself
    pub fn create() -> (Manager, Sender<Request>) {
        let (tx, rx) = mpsc::channel(100);
        (Manager::new(rx), tx)
    }

    /// Listen for new connectios to the manager socket.  
    async fn listen(&mut self) {}
}
