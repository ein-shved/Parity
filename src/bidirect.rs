use super::*;
use crate::streams::*;

use std::{
    collections::{BTreeMap, VecDeque},
    io,
    pin::Pin,
};

use tokio::{
    select,
    sync::{mpsc, oneshot},
};

use futures::{stream::FuturesUnordered, Future, StreamExt};

struct SelfRequest<Data> {
    pub msg: Data,
    pub tx: oneshot::Sender<Result<Data>>,
}

type MessageQueue<Data, SI> = VecDeque<Message<Data, SI>>;
type NoticeQueue<Data> = VecDeque<Data>;
type RequestQueue<Data, SI> = VecDeque<(Data, SI)>;
type InRequestsMap<'a, Data, SI> =
    FuturesUnordered<Pin<Box<dyn Future<Output = Message<Data, SI>> + 'a>>>;
type OutRequestsMap<Data, SI> = BTreeMap<SI, oneshot::Sender<Result<Data>>>;
type RequestProcessor<Data> = Option<mpsc::Sender<RequestImp<Data>>>;
type NoticeProcessor<Data> = Option<mpsc::Sender<Data>>;

pub struct Bidirect<'a, Data: 'a, SI = u16>
where
    SI: SeqId + 'a,
{
    seq_id: SI,

    request_sender: mpsc::Receiver<SelfRequest<Data>>,
    notice_sender: mpsc::Receiver<Data>,
    aborter: mpsc::Receiver<()>,

    request_processor: RequestProcessor<Data>,
    notice_processor: NoticeProcessor<Data>,

    request_sender_user: mpsc::Sender<SelfRequest<Data>>,
    notice_sender_user: mpsc::Sender<Data>,
    aborter_user: mpsc::Sender<()>,

    inbound_requests: InRequestsMap<'a, Data, SI>,
    outgoing_requests: OutRequestsMap<Data, SI>,

    send_queue: MessageQueue<Data, SI>,
    receive_request_queue: RequestQueue<Data, SI>,
    receive_notice_queue: NoticeQueue<Data>,
}

pub struct DefaultRequestProcessor {}

impl<'a, Data: 'a, SI> Bidirect<'a, Data, SI>
where
    SI: SeqId + 'a,
{
    pub fn new() -> Self {
        let (request_sender_user, request_sender) = mpsc::channel::<SelfRequest<Data>>(16);
        let (notice_sender_user, notice_sender) = mpsc::channel::<Data>(16);
        let (aborter_user, aborter) = mpsc::channel::<()>(2);
        Self {
            seq_id: SI::zero(),

            request_sender,
            notice_sender,
            aborter,

            request_processor: Default::default(),
            notice_processor: Default::default(),

            request_sender_user,
            notice_sender_user,
            aborter_user,

            inbound_requests: Default::default(),
            outgoing_requests: Default::default(),

            send_queue: Default::default(),
            receive_request_queue: Default::default(),
            receive_notice_queue: Default::default(),
        }
    }

    pub async fn next(
        &mut self,
        sender: &mut impl MessageSender<Data, SI>,
        receiver: &mut impl MessageReceiver<Data, SI>,
    ) -> Status {
        // TODO(Shvedov): Performance gap here. Move to tasks.
        if let Some(to_send) = self.send_queue.pop_back() {
            return sender.send(to_send).await;
        }
        if let Some(in_request) = self.receive_request_queue.pop_back() {
            let fut = Self::process_request_next(in_request, &mut self.request_processor).await;
            return fut.map(|fut| {
                self.inbound_requests.push(fut);
            });
        }
        if let Some(in_notice) = self.receive_notice_queue.pop_back() {
            return Self::process_notice_next(in_notice, &mut self.notice_processor).await;
        }

        select! {
            msg = receiver.recv() => self.process_next(msg),

            rsp = self.inbound_requests.next(), if !self.inbound_requests.is_empty() =>
            {
                if let Some(rsp) = rsp {
                    self.send_queue.push_front(rsp);
                }
                Ok(())
            }

            req = self.request_sender.recv() => Self::send_request_next(
                    req,
                    &mut self.seq_id,
                    &mut self.send_queue,
                    &mut self.outgoing_requests),

            not = self.notice_sender.recv() => Self::send_notice_next(
                    not,
                    &mut self.send_queue),

            _ = self.aborter.recv() => Result::Err(Error::new(io::ErrorKind::ConnectionAborted, "Aborted by user")),

        }
    }

    fn process_next(&mut self, msg: ResultMessage<Data, SI>) -> Status {
        match msg {
            Ok(msg) => match msg {
                Message::Request(si, data) => self.process_request(si, data),
                Message::Response(si, data) => self.process_response(si, Ok(data)),
                Message::Notice(data) => self.process_notice(data),
                Message::Err(err, si) => self.process_err(err, si),
            },
            Err(err) => Err(err),
        }
    }

    fn process_request(&mut self, si: SI, data: Data) -> Status {
        self.receive_request_queue.push_front((data, si));
        Ok(())
    }

    async fn process_request_next(
        req: (Data, SI),
        processor: &mut RequestProcessor<Data>,
    ) -> Result<Pin<Box<dyn Future<Output = Message<Data, SI>> + 'a>>> {
        let (data, si) = req;
        if let Some(processor) = processor {
            let (tx, rx) = oneshot::channel::<Result<Data>>();
            let req = RequestImp::<Data> {
                data,
                responser: tx,
            };

            processor.send(req).await.unwrap();
            let fut = async move {
                let rsp = rx.await.unwrap();
                match rsp {
                    Ok(rsp) => Message::Response(si, rsp),
                    Err(err) => Message::Err(err, Some(si)),
                }
            };
            Ok(Box::pin(fut))
        } else {
            let fut = async move { Message::not_implemented("").set_si(si) };
            Ok(Box::pin(fut))
        }
    }

    fn process_notice(&mut self, data: Data) -> Status {
        self.receive_notice_queue.push_front(data);
        Ok(())
    }

    async fn process_notice_next(data: Data, processor: &mut NoticeProcessor<Data>) -> Status {
        if let Some(processor) = processor {
            // TODO(Shvedov): Process result
            processor.send(data).await.unwrap();
        }
        Ok(())
    }

    fn process_response(&mut self, si: SI, data: Result<Data>) -> Status {
        if let Some(waiter) = self.outgoing_requests.remove(&si) {
            // TODO(Shvedov): Process result
            waiter.send(data);
        }
        Ok(())
    }

    fn process_err(&mut self, data: Error, si: Option<SI>) -> Status {
        if let Some(si) = si {
            self.process_response(si, Err(data))
        } else {
            // TODO(Shvedov): Notify user about error
            Ok(())
        }
    }

    fn send_request_next(
        req: Option<SelfRequest<Data>>,
        si: &mut SI,
        send_queue: &mut MessageQueue<Data, SI>,
        requests_map: &mut OutRequestsMap<Data, SI>,
    ) -> Status {
        if let Some(req) = req {
            send_queue.push_front(Message::Request(*si, req.msg));
            requests_map.insert(*si, req.tx);
            *si = (*si).inc();
        }
        Ok(())
    }

    fn send_notice_next(not: Option<Data>, send_queue: &mut MessageQueue<Data, SI>) -> Status {
        if let Some(not) = not {
            send_queue.push_front(Message::Notice(not));
        }
        Ok(())
    }
}

struct RequestImp<Data> {
    data: Data,
    responser: oneshot::Sender<Result<Data>>,
}

impl<Data> Request<Data> for RequestImp<Data> {
    fn get_data(&self) -> &Data {
        &self.data
    }
    async fn response(self, response: Result<Data>) -> Status {
        self.responser.send(response); // TODO .expect("Failed to send response");
        Ok(())
    }
}

struct RequestSenderImpl<Data> {
    channel: mpsc::Sender<SelfRequest<Data>>,
}

impl<Data> RequestSender<Data> for RequestSenderImpl<Data> {
    async fn request(&mut self, req: Data) -> Result<Data> {
        let (tx, rx) = oneshot::channel();
        let req = SelfRequest { msg: req, tx };
        self.channel.send(req).await.unwrap();
        rx.await.unwrap()
    }
}

#[tokio::test]
async fn get_request_sender() {
    let mut bidir = Bidirect::<String>::new();
    let mut sender = bidir.get_request_sender();
    _ = sender.request(String::from("Hello world!"));
}

impl<'b, Data: 'b, SI: SeqId> BidirectStream<'b, Data> for Bidirect<'_, Data, SI> {
    fn get_request_sender(&mut self) -> impl RequestSender<Data> + 'b {
        RequestSenderImpl {
            channel: self.request_sender_user.clone(),
        }
    }
}
