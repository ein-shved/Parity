use super::utils::*;
use async_trait::async_trait;
use std::marker::PhantomData;

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

pub trait MessageBuilder<SI: SeqId, Data, RawData> {
    fn build(&self, msg: Message<SI, Data>) -> Result<RawData>;
}

pub trait MessageParser<SI: SeqId, Data, RawData> {
    fn parse(&self, raw: RawData) -> ResultMessage<SI, Data>;
}

pub struct MessageStreamSender<BuilderT, SenderT, RawData> {
    builder: BuilderT,
    sender: SenderT,
    phantom: PhantomData<RawData>,
}

impl<SI, Data, RawData, BuilderT, SenderT> MessageSender<SI, Data>
    for MessageStreamSender<BuilderT, SenderT, RawData>
where
    SI: SeqId,
    BuilderT: MessageBuilder<SI, Data, RawData>,
    SenderT: Sender<RawData>,
{
    async fn send(&mut self, msg: Message<SI, Data>) -> Status {
        self.sender.send(self.builder.build(msg)?).await
    }
}

impl<BuilderT, SenderT, RawData> MessageStreamSender<BuilderT, SenderT, RawData> {
    pub fn new(builder: BuilderT, sender: SenderT) -> Self {
        Self {
            builder,
            sender,
            phantom: <PhantomData<RawData> as Default>::default(),
        }
    }
}

pub struct MessageStreamReceiver<ParserT, ReceiverT, RawData> {
    parser: ParserT,
    reciever: ReceiverT,
    phantom: PhantomData<RawData>,
}

impl<SI, Data, RawData, ParserT, ReceiverT> MessageReceiver<SI, Data>
    for MessageStreamReceiver<ParserT, ReceiverT, RawData>
where
    SI: SeqId,
    ParserT: MessageParser<SI, Data, RawData>,
    ReceiverT: Receiver<RawData>,
{
    async fn recv(&mut self) -> ResultMessage<SI, Data> {
        self.parser.parse(self.reciever.recv().await?)
    }
}
