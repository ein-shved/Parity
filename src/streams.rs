use super::utils::*;
use std::future::Future;

pub trait Sender<RawData> {
    fn send(&mut self, msg: RawData) -> impl Future<Output = Status> + Send;
}

pub trait Receiver<RawData> {
    fn recv(&mut self) -> impl Future<Output = Result<RawData>> + Send;
}

pub trait MessageSender<Data, SI: SeqId> {
    fn send(&mut self, msg: Message<Data, SI>) -> impl Future<Output = Status> + Send;
}

pub trait MessageReceiver<Data, SI: SeqId> {
    fn recv(&mut self) -> impl Future<Output = ResultMessage<Data, SI>> + Send;
}
