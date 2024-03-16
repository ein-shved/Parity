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

#[async_trait]
pub trait MessageSender<Data, SI: SeqId> {
    async fn send(&mut self, msg: Message<Data, SI>) -> Status;
}

#[async_trait]
pub trait MessageReceiver<Data, SI: SeqId> {
    async fn recv(&mut self) -> ResultMessage<Data, SI>;
}

#[async_trait]
impl<Data: Send + 'static, SI: SeqId + Send + 'static, T> MessageSender<Data, SI> for T
    where T: Sender<Message<Data, SI>> + Send
{
    async fn send(&mut self, msg: Message<Data, SI>) -> Status
    {
        (self as &mut T).send(msg).await
    }
}

#[async_trait]
impl<Data, SI: SeqId, T> MessageReceiver<Data, SI> for T
    where T: Receiver<Message<Data, SI>> + Send
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

#[async_trait]
impl<Data, RawData, SI, BuilderT, SenderT> MessageSender<Data, SI>
    for MessageStreamSender<BuilderT, SenderT, RawData>
where
    Data: 'static + Send,
    RawData: Send,
    SI: SeqId + 'static + Send,
    BuilderT: MessageBuilder<Data, RawData, SI> + Send,
    SenderT: Sender<RawData> + Send,
{
    async fn send(&mut self, msg: Message<Data, SI>) -> Status {
        let data = self.builder.build(msg);
        self.sender.send(data?).await
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

#[async_trait]
impl<Data, RawData, SI, ParserT, ReceiverT> MessageReceiver<Data, SI>
    for MessageStreamReceiver<ParserT, ReceiverT, RawData>
where
    Data: Send,
    RawData: Send,
    SI: SeqId + Send,
    ParserT: MessageParser<Data, RawData, SI> + Sync + Send,
    ReceiverT: Receiver<RawData> + Send,
{
    async fn recv(&mut self) -> ResultMessage<Data, SI> {
        self.parser.parse(self.reciever.recv().await?)
    }
}
