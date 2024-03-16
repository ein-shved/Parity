use num::{Num, Zero};
use std::io;

// Top-level type aliases
pub type Error = io::Error;
pub type Result<T> = io::Result<T>;

// Reuse type aliases
pub type Status = Result<()>;
pub type ResultMessage<SI, Data> = Result<Message<SI, Data>>;

// Local types
pub enum Message<SI: SeqId, Data> {
    Request(SI, Data),
    Response(SI, Data),
    Notice(Data),
    Err(Error, Option<SI>),
}

impl<SI: SeqId, Data> Message<SI, Data> {
    pub fn set_si(self, si: SI) -> Self {
        match self {
            Self::Request(_, data) => Self::Request(si, data),
            Self::Response(_, data) => Self::Request(si, data),
            Self::Notice(_) => self,
            Self::Err(err, _) => Self::Err(err, Some(si)),
        }
    }
}

pub trait SeqId: Zero + PartialEq + Ord {
    fn inc(self) -> Self;
}

impl<T: Num + Ord> SeqId for T {
    fn inc(self) -> Self {
        return self + Self::one();
    }
}

// Help traits
pub trait ErrorBuilder {
    fn not_implemented(what: &str) -> Self;
}

impl<SI: SeqId, Data> ErrorBuilder for Message<SI, Data> {
    fn not_implemented(what: &str) -> Self {
        Self::Err(io::Error::new(io::ErrorKind::Unsupported, what), None)
    }
}
