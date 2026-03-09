use std::fmt;

/// Context from the initialize handshake, passed to every handler.
#[derive(Debug, Clone)]
pub struct Context {
    pub protocol_version: u32,
    pub profile: String,
    pub dry_run: bool,
}

/// Ok(()) sends ack. Err(message) sends error response.
pub type HandlerResult = Result<(), String>;

/// SDK error type for I/O or protocol failures.
pub enum Error {
    Io(std::io::Error),
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => f.debug_tuple("Io").field(e).finish(),
            Error::Protocol(msg) => f.debug_tuple("Protocol").field(msg).finish(),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Protocol(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
