use crate::{protocol::prelude::*, utils::error};
use futures::{SinkExt, StreamExt};
use std::io;
use tokio::net::TcpSocket;

pub struct Client {
    /// The address that the client must connect to
    address: String,
}

impl Client {
    /// A test function to see if the server works correctly
    pub async fn run(&self) -> io::Result<()> {
        let addr = self.address.parse().unwrap();
        let socket = TcpSocket::new_v4()?;
        let stream = socket.connect(addr).await?;
        let (mut tx, _) = Framed::new(stream, FrameCodec).split();
        println!("Connected to socket at {}", self.address);
        for i in 0..100 {
            println!("Sending {}", i);
            let message = String::from("hello world");
            let bytes: Vec<u8> = message.as_bytes().to_vec();
            tx.send(Frame {
                stream: i,
                length: bytes.len() as u64,
                contents: bytes,
            })
            .await
            .map_err(|_| error("could not send"))?;
        }

        todo!()
    }

    pub fn new(addr: impl Into<String>) -> Client {
        Client {
            address: addr.into(),
        }
    }
}
