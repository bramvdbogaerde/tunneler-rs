use tokio::{
    net::{TcpListener, TcpStream},
    sync::{
        mpsc,
        mpsc::{Receiver, Sender},
        Mutex,
    },
};

use tokio_util::codec::{Decoder, Framed};

use crate::utils::error;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::BytesMut;
use futures::{prelude::*, stream::StreamExt};
use std::{
    collections::HashMap,
    io,
    io::{Cursor, Read},
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

#[derive(Debug, Clone)]
struct Frame {
    /// the stream number, used to multiplex connections
    stream: u64,
    /// the length of the frame (in bytes, except for the header of the frame)
    length: u64,
    /// the contents of the frame
    contents: Vec<u8>,
}

struct FrameDecoder;

impl Decoder for FrameDecoder {
    type Item = Frame;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let src_length = src.len();
        let mut cursor = Cursor::new(src);
        let stream: u64 = if src_length >= 8 {
            cursor
                .read_u64::<BigEndian>()
                .map_err(|_| error("could not read stream id"))?
        } else {
            return Ok(None);
        };

        let length: u64 = if src_length >= 16 {
            cursor
                .read_u64::<BigEndian>()
                .map_err(|_| error("could not read length of frame"))?
        } else {
            return Ok(None);
        };

        let contents: Vec<u8> = if src_length as u64 >= 16 + length {
            let mut output = vec![0 as u8; length as usize];
            cursor.read_exact(&mut output)?;
            output
        } else {
            return Ok(None);
        };

        Ok(Some(Frame {
            stream,
            length,
            contents,
        }))
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
        let mut stream = Framed::new(socket, FrameDecoder);
        // TODO: do some clean up when stream closes
        loop {
            let frame: Frame = stream
                .next()
                .await
                .ok_or_else(|| error("stream closed"))??;

            todo!()
        }
        Ok(())
    }

    pub async fn listen(&self) -> io::Result<()> {
        // TODO: make the bind host and port configurable
        let listener = TcpListener::bind("127.0.0.1:5555").await?;
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
