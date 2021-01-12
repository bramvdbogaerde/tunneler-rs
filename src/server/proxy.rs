use crate::server::{ManagerRef, Request};
use crate::{http_parser::HttpRequest, utils::error};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct ProxyServer {
    port: usize,
    host: String,
    request_sender: ManagerRef,
}

impl ProxyServer {
    pub fn new<S: Into<String>>(port: usize, host: S, request_sender: ManagerRef) -> ProxyServer {
        ProxyServer {
            port,
            host: host.into(),
            request_sender,
        }
    }

    async fn process(manager: ManagerRef, mut socket: TcpStream) -> std::io::Result<()> {
        let request = HttpRequest::parse(&mut socket).await?;
        let host = match request.headers.get("Host") {
            Some(h) => h,
            // we simply ignore the request if it does not have a valid host header
            None => return Ok(()),
        };

        let (tx, mut rx) = mpsc::channel(1);
        let request = Request {
            id: host.clone(),
            contents: request.bytes,
            reply_to: tx,
        };

        manager
            .submit_request(request)
            .await
            .map_err(|_| error("could not send request"))?;

        let response = rx.recv().await.ok_or(error("could not receive request"))?;

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
