use super::utils::*;
use async_trait::async_trait;
use std::marker::PhantomData;

pub trait Sender<RawData> {
    async fn send(&mut self, msg: RawData) -> Status;
}

pub trait Receiver<RawData> {
    async fn recv(&mut self) -> Result<RawData>;
}

pub trait MessageSender<Data, SI: SeqId> {
    async fn send(&mut self, msg: Message<Data, SI>) -> Status;
}

pub trait MessageReceiver<Data, SI: SeqId> {
    async fn recv(&mut self) -> ResultMessage<Data, SI>;
}

impl<Data, SI: SeqId, T> MessageSender<Data, SI> for T
    where T: Sender<Message<Data, SI>>
{
    async fn send(&mut self, msg: Message<Data, SI>) -> Status
    {
        (self as &mut T).send(msg).await
    }
}

impl<Data, SI: SeqId, T> MessageReceiver<Data, SI> for T
    where T: Receiver<Message<Data, SI>>
{
    async fn recv(&mut self) -> ResultMessage<Data, SI>
    {
        (self as &mut T).recv().await
    }
}

pub trait MessageBuilder<Data, RawData, SI: SeqId> {
    fn build(&self, msg: Message<Data, SI>) -> Result<RawData>;
}

pub trait MessageParser<Data, RawData, SI: SeqId> {
    fn parse(&self, raw: RawData) -> ResultMessage<Data, SI>;
}

pub struct MessageStreamSender<BuilderT, SenderT, RawData> {
    builder: BuilderT,
    sender: SenderT,
    phantom: PhantomData<RawData>,
}

impl<Data, RawData, SI, BuilderT, SenderT> MessageSender<Data, SI>
    for MessageStreamSender<BuilderT, SenderT, RawData>
where
    SI: SeqId,
    BuilderT: MessageBuilder<Data, RawData, SI>,
    SenderT: Sender<RawData>,
{
    async fn send(&mut self, msg: Message<Data, SI>) -> Status {
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

impl<Data, RawData, SI, ParserT, ReceiverT> MessageReceiver<Data, SI>
    for MessageStreamReceiver<ParserT, ReceiverT, RawData>
where
    SI: SeqId,
    ParserT: MessageParser<Data, RawData, SI>,
    ReceiverT: Receiver<RawData>,
{
    async fn recv(&mut self) -> ResultMessage<Data, SI> {
        self.parser.parse(self.reciever.recv().await?)
    }
}
