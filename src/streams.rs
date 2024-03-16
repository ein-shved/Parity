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

pub trait MessageSender<SI: SeqId, Data> {
    async fn send(&mut self, msg: Message<SI, Data>) -> Status;
}

pub trait MessageReceiver<SI: SeqId, Data> {
    async fn recv(&mut self) -> ResultMessage<SI, Data>;
}
