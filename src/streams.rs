use super::utils::*;
use async_trait::async_trait;

#[async_trait]
pub trait Sender<RawData> {
    async fn send(&mut self, msg: RawData) -> Status;
}

#[async_trait]
pub trait Receiver<RawData> {
    async fn recv(&mut self) -> Result<RawData>;
}

pub trait MessageSender<Data, SI: SeqId> {
    async fn send(&mut self, msg: Message<Data, SI>) -> Status;
}

pub trait MessageReceiver<Data, SI: SeqId> {
    async fn recv(&mut self) -> ResultMessage<Data, SI>;
}
