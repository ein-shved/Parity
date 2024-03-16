use super::utils::*;
use std::{future::Future, marker::PhantomData};

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

impl<Data, SI: SeqId, T> MessageSender<Data, SI> for T
where
    T: Sender<Message<Data, SI>>,
{
    fn send(&mut self, msg: Message<Data, SI>) -> impl Future<Output = Status> + Send {
        (self as &mut T).send(msg)
    }
}

impl<Data, SI: SeqId, T> MessageReceiver<Data, SI> for T
where
    T: Receiver<Message<Data, SI>>,
{
    fn recv(&mut self) -> impl Future<Output = ResultMessage<Data, SI>> + Send {
        (self as &mut T).recv()
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
    RawData: Send,
    BuilderT: MessageBuilder<Data, RawData, SI>,
    SenderT: Sender<RawData> + Send,
{
    fn send(&mut self, msg: Message<Data, SI>) -> impl Future<Output = Status> + Send {
        let data = self.builder.build(msg);
        async {
            self.sender.send(data?).await
        }
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
    ParserT: MessageParser<Data, RawData, SI> + Sync,
    ReceiverT: Receiver<RawData>,
{
    fn recv(&mut self) -> impl Future<Output = ResultMessage<Data, SI>> + Send {
        let fut = self.reciever.recv();
        async {
            self.parser.parse(fut.await?)
        }
    }
}
