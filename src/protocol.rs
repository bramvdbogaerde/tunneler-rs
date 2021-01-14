use crate::utils::error;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::{Buf, BufMut, BytesMut};
use std::{
    io,
    io::{Cursor, Read},
};
use tokio_util::codec::{Decoder, Encoder};

/// The smallest subdivision for our packets, runs over TCP.
#[derive(Debug, Clone)]
pub struct Frame {
    /// the stream number, used to multiplex connections
    pub stream: u64,
    /// the length of the frame (in bytes, except for the header of the frame)
    pub length: u64,
    /// the contents of the frame
    pub contents: Vec<u8>,
}

/// This codec allows for the decoding and encoding of arbitrary TCP packets into frames
pub struct FrameCodec;

macro_rules! read_bytes {
    ($len:expr, $length:expr, $expression:expr, $message:expr) => {
        if $len >= $length {
            $expression.map_err(|_| error($message))?
        } else {
            return Err(crate::utils::error($message));
        }
    };
}

macro_rules! read_bytes64 {
    ($len:expr, $length:expr, $cursor:expr, $message:expr) => {
        read_bytes!(
            $len,
            $length,
            $cursor.read_u64::<byteorder::BigEndian>(),
            $message
        )
    };
}

impl Decoder for FrameCodec {
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

        // advance the pointer in the buffer with the size of the frame + size of the header
        let bytes = cursor.into_inner();
        bytes.advance(16 + length as usize);

        Ok(Some(Frame {
            stream,
            length,
            contents,
        }))
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = io::Error;
    fn encode(&mut self, frame: Frame, buff: &mut bytes::BytesMut) -> Result<(), io::Error> {
        buff.put_u64(frame.stream);
        buff.put_u64(frame.length);
        buff.put(frame.contents.as_slice());
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    /// This message is sent from the client to the server, in order to setup a proxy on some
    /// subdomain, and start accepting connections from that domain.
    ///
    /// This packet should be sent over stream 0.
    ///
    /// Layout in bytes:
    ///
    /// * type (1 byte): set to 0x0
    /// * contains_subdomain (1 byte): 0x0 if not, 0x1 otherwise
    /// * subdomain_length (8 byte): big endian length of the subdomain string
    /// * subdomain (aribrary length)
    RequestProxy {
        /// The name of the custom subdomain
        custom_subdomain: Option<String>,
    },

    /// Sent from the server to the client when someone connects to the proxy.
    ///
    /// Layout in bytes:
    ///
    /// * type (1 byte): set to 0x1
    /// * stream_id (8 bytes): big endian stream id
    InitializeConnection {
        /// The id of the stream that the server is going to use to send forwarded TCP packets,
        /// and which the client should use to reply.
        stream_id: u64,
    },

    /// Sent by the client or server, depending on what side the tcp connection was closed,
    /// causes the TCP connection of the client, with the proxied server to close if sent
    /// from the server, or cause the connection from the server with  external clients to close
    /// if coming from the client.
    ///
    /// Should be sent over stream 0.
    ///
    /// Layout in bytes:
    ///
    /// * type (1 byte): set to 0x2
    /// * stream_id (8 bytes): big endian stream id to clsoe
    ///
    CloseConnection {
        ///  The id of the stream that shall be closed on the server or client.
        stream_id: u64,
    },

    /// A packet sent from either server or client which was proxied
    ///
    /// Layout in bytes:
    /// * type (1 byte): set to 0x3
    /// * contents (arbitrary lenght)
    OverlayedPacket { contents: Vec<u8> },
}

impl Packet {
    /// Parses a packet from the given stream of bytes
    pub fn parse(bytes: Vec<u8>) -> Result<Packet, io::Error> {
        let len = bytes.len();
        println!("parsing {:?}", bytes);
        let mut cursor = Cursor::new(bytes);
        // type
        let packet_type = read_bytes!(len, 1, cursor.read_u8(), "could not read packet type");

        let packet = match packet_type {
            // request proxy
            0x0 => {
                // contains subdomain
                let contains_subdomain = read_bytes!(
                    len,
                    2,
                    cursor.read_u8(),
                    "could not read contains subdomain"
                );

                if contains_subdomain == 0x01 {
                    let length = read_bytes64!(len, 10, cursor, "could not read subdomain length");
                    let mut buff = vec![0 as u8; length as usize];
                    cursor.read_exact(&mut buff)?;

                    Packet::RequestProxy {
                        custom_subdomain: Some(
                            String::from_utf8(buff)
                                .map_err(|_| error("could not parse subdomain string"))?,
                        ),
                    }
                } else {
                    Packet::RequestProxy {
                        custom_subdomain: None,
                    }
                }
            }

            // initialize connection
            0x01 => {
                let stream_id = read_bytes64!(len, 9, cursor, "could not read stream id");
                Packet::InitializeConnection { stream_id }
            }

            // close connection
            0x02 => {
                let stream_id = read_bytes64!(len, 9, cursor, "could not read stream id");
                Packet::CloseConnection { stream_id }
            }

            // overlay packet
            0x03 => {
                let mut buff = vec![0 as u8; len - 1];
                cursor.read_exact(&mut buff)?;

                Packet::OverlayedPacket { contents: buff }
            }

            _ => return Err(error("invalid packet type")),
        };

        Ok(packet)
    }

    /// Converts a packet to a series of bytes that can be sent over a frame
    pub fn serialize(&self) -> Result<Vec<u8>, io::Error> {
        // we are going to initialize a sufficiently large buffer, for efficiency
        // the largest packet is (at minimum) 10 bytes long.
        let mut buff: Vec<u8> = Vec::with_capacity(10);

        use Packet::*;

        match self {
            RequestProxy {
                custom_subdomain: None,
            } => {
                buff.push(0x0); // type
                buff.push(0x0); // contains_subdomain
            }

            RequestProxy {
                custom_subdomain: Some(domain),
            } => {
                buff.push(0x0); // type
                buff.push(0x01); // contains_subdomain
                let contents = domain.as_bytes();
                buff.write_u64::<BigEndian>(contents.len() as u64)
                    .map_err(|_| error("could not write bytes"))?; // length
                buff.extend(contents); // domain itself
            }

            InitializeConnection { stream_id } => {
                buff.push(0x01); // type
                buff.write_u64::<BigEndian>(*stream_id)
                    .map_err(|_| error("could not write bytes"))?; // stream_id
            }

            CloseConnection { stream_id } => {
                buff.push(0x02);
                buff.write_u64::<BigEndian>(*stream_id)
                    .map_err(|_| error("could not write bytes"))?;
            }

            OverlayedPacket { contents } => {
                buff.push(0x03);
                buff.extend(contents);
            }
        }

        Ok(buff)
    }
}

#[cfg(test)]
mod tests {
    use super::Packet;
    #[test]
    fn test_initialize_connection() {
        let packet = Packet::InitializeConnection { stream_id: 1 };

        let bytes = packet.serialize().unwrap();
        let parsed = Packet::parse(bytes).unwrap();
        assert_eq!(packet, parsed);
    }

    #[test]
    fn test_close_connection() {
        let packet = Packet::CloseConnection { stream_id: 1 };

        let bytes = packet.serialize().unwrap();
        let parsed = Packet::parse(bytes).unwrap();
        assert_eq!(packet, parsed);
    }

    #[test]
    fn test_request_proxy() {
        let packet_subdomain = Packet::RequestProxy {
            custom_subdomain: Some("foobar".to_string()),
        };

        let packet = Packet::RequestProxy {
            custom_subdomain: None,
        };

        let bytes_subdomain = packet_subdomain.serialize().unwrap();
        let parsed_subdomain = Packet::parse(bytes_subdomain).unwrap();

        let bytes = packet.serialize().unwrap();
        let parsed = Packet::parse(bytes).unwrap();

        assert_eq!(packet_subdomain, parsed_subdomain);
        assert_eq!(packet, parsed);
    }

    #[test]
    fn test_overlayed_packet() {
        let packet = Packet::OverlayedPacket {
            contents: vec![0, 20, 244, 10, 0],
        };

        let bytes = packet.serialize().unwrap();
        let parsed = Packet::parse(bytes).unwrap();
        assert_eq!(packet, parsed);
    }
}

pub mod prelude {
    pub use super::{Frame, FrameCodec};
    pub use tokio_util::codec::{Decoder, Encoder, Framed};
}
