use num::{Num, Zero};
use std::io;

// Top-level type aliases
pub type Error = io::Error;
pub type Result<T> = io::Result<T>;

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

pub trait SeqId: Zero + PartialEq + Ord + Copy {
    fn inc(self) -> Self;
}

impl<T: Num + Ord + Copy> SeqId for T {
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
        Self::Err(io::Error::new(io::ErrorKind::Unsupported, what), None)
    }
}
