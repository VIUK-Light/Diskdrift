use std::fmt;

/// Minimal error type for DiskDrift. No external error-handling crates.
#[derive(Debug)]
pub enum Error {
    Message(String),
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Message(m) => write!(f, "{m}"),
            Error::Io(e) => write!(f, "{e}"),
            Error::Sqlite(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Sqlite(e)
    }
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Message(e)
    }
}

impl From<&str> for Error {
    fn from(e: &str) -> Self {
        Error::Message(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
