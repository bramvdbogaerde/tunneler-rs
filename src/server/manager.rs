use tokio::{
    net::{TcpListener, TcpStream},
    sync::{
        mpsc,
        mpsc::{Receiver, Sender},
        Mutex,
    },
};

use crate::protocol::prelude::*;
use crate::utils::error;
use futures::stream::StreamExt;
use std::{
    collections::HashMap,
    io,
    sync::Arc,
};


#[derive(Debug, Clone)]
struct Connection;

#[derive(Debug, Clone)]
pub struct Request {
    pub id: String,
    pub contents: Vec<u8>,
    pub reply_to: Sender<Response>,
}

pub struct Response {
    pub contents: Vec<u8>,
}

pub enum ManagerError {
    SubmitRequestFailure,
}

#[derive(Clone, Debug)]
pub struct ManagerRef {
    sender: Sender<Request>,
}

impl ManagerRef {
    fn new(sender: Sender<Request>) -> ManagerRef {
        ManagerRef { sender }
    }

    pub async fn submit_request(&self, request: Request) -> Result<(), ManagerError> {
        self.sender
            .send(request)
            .await
            .map_err(|_| ManagerError::SubmitRequestFailure)?;

        Ok(())
    }
}

#[derive(Debug)]
struct ManagerInner {
    /// A mapping from automatically generated names to connections
    connections: Mutex<HashMap<String, Connection>>,
    /// A channel on which requests will arrive
    requests_receiver: Receiver<Request>,
}

impl ManagerInner {
    fn new(receiver: Receiver<Request>) -> ManagerInner {
        ManagerInner {
            connections: Mutex::new(Default::default()),
            requests_receiver: receiver,
        }
    }
}

/// Structure holds all the connection, and mappings between IDs and connections
#[derive(Clone, Debug)]
pub struct Manager {
    inner: Arc<ManagerInner>,
}

impl Manager {
    pub fn new(requests_receiver: Receiver<Request>) -> Manager {
        Manager {
            inner: Arc::new(ManagerInner::new(requests_receiver)),
        }
    }

    /// Same as new but creates an mpsc channel itself
    pub fn create() -> (Manager, ManagerRef) {
        let (tx, rx) = mpsc::channel(100);
        (Manager::new(rx), ManagerRef::new(tx))
    }

    async fn process(&self, socket: TcpStream) -> io::Result<()> {
        let mut stream = Framed::new(socket, FrameCodec);
        // TODO: do some clean up when stream closes
        loop {
            let frame: Frame = stream
                .next()
                .await
                .ok_or_else(|| error("stream closed"))??;

            println!("Got a frame {:?}", frame);
            println!("The message was {}", String::from_utf8(frame.contents).unwrap())

        }
    }

    pub async fn listen(&self) -> io::Result<()> {
        // TODO: make the bind host and port configurable
        let listener = TcpListener::bind("127.0.0.1:5555").await?;
        println!("Starting to listen for connections on localhost:5555");
        loop {
            let (socket, _) = listener.accept().await?;
            let this = self.clone();
            tokio::spawn(async move {
                this.process(socket)
                    .await
                    .expect("processing socket failed");
            });
        }
    }
}
