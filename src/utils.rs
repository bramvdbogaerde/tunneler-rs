use std::io;

pub fn error<S: Into<String>>(err_msg: S) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err_msg.into())
}
