use num::{Num, Zero};
use std::{io, fmt};
use tokio::{sync::mpsc::error::SendError, task::JoinError};

// Top-level type aliases
pub struct Error(pub io::Error);

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn new(kind: io::ErrorKind, what: &str) -> Self
    {
        Error(io::Error::new(kind, what))
    }

    pub fn pipe(what: &str) -> Self
    {
        Self(io::Error::new(io::ErrorKind::BrokenPipe, what))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// Reuse type aliases
pub type Status = Result<()>;
pub type ResultMessage<Data, SI> = Result<Message<Data, SI>>;

// Local types
pub enum Message<Data, SI: SeqId> {
    Request(SI, Data),
    Response(SI, Data),
    Notice(Data),
    Err(Error, Option<SI>),
}

impl<Data, SI: SeqId> Message<Data, SI> {
    pub fn set_si(self, si: SI) -> Self {
        match self {
            Self::Request(_, data) => Self::Request(si, data),
            Self::Response(_, data) => Self::Request(si, data),
            Self::Notice(_) => self,
            Self::Err(err, _) => Self::Err(err, Some(si)),
        }
    }
}

pub trait SeqId: Zero + PartialEq + Ord + Copy + fmt::Display + Send + 'static {
    fn inc(self) -> Self;
}

impl<T: Num + Ord + Copy + Send + fmt::Display + 'static> SeqId for T {
    fn inc(self) -> Self {
        return self + Self::one();
    }
}

// Help traits
pub trait ErrorBuilder {
    fn not_implemented(what: &str) -> Self;
}

impl<Data, SI: SeqId> ErrorBuilder for Message<Data, SI> {
    fn not_implemented(what: &str) -> Self {
        Self::Err(
            Error(io::Error::new(io::ErrorKind::Unsupported, what)),
            None)
    }
}

impl<T> From<SendError<T>> for Error {
    fn from(value: SendError<T>) -> Self {
        Self::pipe(&value.to_string())
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self(value)
    }
}

impl From<JoinError> for Error {
    fn from(value: JoinError) -> Self {
            Self::pipe(&format!("Failed to join: {}", value))
    }
}
