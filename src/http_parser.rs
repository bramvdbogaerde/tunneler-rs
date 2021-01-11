/// A very primitive HTTP parser, that is only capable of parsing the headers of an HTTP requests
/// into a HashMap of String -> String
use std::{collections::HashMap, io};
use tokio::io::{AsyncRead, AsyncReadExt};

pub struct HttpRequest {
    /// The headers of the request
    pub headers: HashMap<String, String>,
    /// The bytes that we have been reading
    pub bytes: Vec<u8>,
}

impl HttpRequest {
    fn empty() -> HttpRequest {
        HttpRequest {
            headers: HashMap::new(),
            bytes: Vec::new(),
        }
    }

    /// Parses the HTTP request until all headers have been received,
    /// then returns the partially parsed http request.
    pub async fn parse(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<HttpRequest> {
        let mut buffer = [0 as u8; 100];
        let mut request = HttpRequest::empty();

        let header_size = loop {
            let n = stream.read(&mut buffer).await?;
            let scan: Vec<u8> = vec!['\r', '\n', '\r', '\n']
                .into_iter()
                .map(|v| v as u8)
                .collect();

            let done = buffer[0..n]
                .windows(scan.len())
                .position(|window| window == scan);

            request.bytes.append(&mut buffer[0..n].to_vec());
            if done.is_some() {
                break request.bytes.len() - n + done.unwrap() + 4
            }
        };

        let headers = String::from_utf8(request.bytes[0..header_size-4].to_vec().clone())
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "could not parse headers"))?;

        println!("{:?}", headers.split("\r\n").collect::<Vec<&str>>());
        request.headers = headers
            .split("\r\n")
            .map(|header| {
                let mut kv = header.split(":").map(|v| v.to_string());
                (
                    kv.next().unwrap_or("".to_string()),
                    kv.next().unwrap_or("".to_string()),
                )
            })
            .collect();


        let content_length = request
            .headers
            .get("Content-Length")
            .map(|v| v.trim().parse::<usize>().ok())
            .flatten()
            .unwrap_or(0);


        // TODO: check for content length so that we do not read forever
    
        let mut buff = [0 as u8; 100];
        let mut bytes_read = request.bytes.len() - header_size;
        while bytes_read < content_length {
            let n = stream.read(&mut buff).await?;
            bytes_read += n;
            request.bytes.append(&mut buff[0..n].to_vec());
        }

        Ok(request)
    }
}
