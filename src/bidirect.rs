use std::{
    collections::{BTreeMap, VecDeque},
    io,
};

use super::*;
use crate::streams::*;

use tokio::{
    select,
    sync::{mpsc, oneshot},
};

struct SelfRequest<Data> {
    pub msg: Data,
    pub tx: oneshot::Sender<Result<Data>>,
}

type MessageQueue<SI, Data> = VecDeque<Message<SI, Data>>;
type NoticeQueue<Data> = VecDeque<Data>;
type RequestQueue<SI, Data> = VecDeque<(SI, Data)>;
type InRequestsMap<SI, Data> = BTreeMap<SI, oneshot::Receiver<Result<Data>>>;
type OutRequestsMap<SI, Data> = BTreeMap<SI, oneshot::Sender<Result<Data>>>;
type RequestProcessor<Data> = Option<mpsc::Sender<RequestImp<Data>>>;
type NoticeProcessor<Data> = Option<mpsc::Sender<Data>>;

pub struct Bidirect<Data, SI = u16>
where
    SI: SeqId,
{
    seqId: SI,

    request_sender: mpsc::Receiver<SelfRequest<Data>>,
    notice_sender: mpsc::Receiver<Data>,

    request_processor: RequestProcessor<Data>,
    notice_processor: NoticeProcessor<Data>,

    request_sender_user: mpsc::Sender<SelfRequest<Data>>,
    notice_sender_user: mpsc::Sender<Data>,

    inbound_requests: InRequestsMap<SI, Data>,
    outgoing_requests: OutRequestsMap<SI, Data>,

    send_queue: MessageQueue<SI, Data>,
    receive_request_queue: RequestQueue<SI, Data>,
    receive_notice_queue: NoticeQueue<Data>,
}

pub struct DefaultRequestProcessor {}

impl<Data, SI> Bidirect<Data, SI>
where
    SI: SeqId,
{
    pub fn new() -> Self {
        let (request_sender_user, request_sender) = mpsc::channel::<SelfRequest<Data>>(16);
        let (notice_sender_user, notice_sender) = mpsc::channel::<Data>(16);
        Self {
            seqId: SI::zero(),

            request_sender,
            notice_sender,

            request_processor: Default::default(),
            notice_processor: Default::default(),

            request_sender_user,
            notice_sender_user,

            inbound_requests: Default::default(),
            outgoing_requests: Default::default(),

            send_queue: Default::default(),
            receive_request_queue: Default::default(),
            receive_notice_queue: Default::default(),
        }
    }

    pub async fn next<Sender, Receiver>(
        &mut self,
        sender: &mut Sender,
        receiver: &mut Receiver,
    ) -> Status
    where
        Sender: MessageSender<SI, Data>,
        Receiver: MessageReceiver<SI, Data>,
    {
        let to_send = self.send_queue.pop_back();
        let in_request = self.receive_request_queue.pop_back();
        let in_notice = self.receive_notice_queue.pop_back();
        select! {
            res = sender.send(to_send.unwrap()), if to_send.is_some() => res,
            msg = receiver.recv() => self.process_next(msg),
            res = Self::process_request_next(
                    in_request,
                    &mut self.request_processor,
                    &mut self.inbound_requests), if in_request.is_some() =>
                res.map(|msg|{
                    if let Some(msg) = msg {
                        self.send_queue.push_front(msg);
                    }
                }),
            res = Self::process_notice_next(
                    in_notice,
                    &mut self.notice_processor), if in_request.is_some() => res,
        }
    }

    fn process_next(&mut self, msg: ResultMessage<SI, Data>) -> Status {
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
        self.receive_request_queue.push_front((si, data));
        Ok(())
    }

    async fn process_request_next(
        req: Option<(SI, Data)>,
        processor: &mut RequestProcessor<Data>,
        store: &mut InRequestsMap<SI, Data>,
    ) -> Result<Option<Message<SI, Data>>> {
        if let Some((si, data)) = req {
            if let Some(processor) = processor {
                let (tx, rx) = oneshot::channel::<Result<Data>>();
                let req = RequestImp::<Data> {
                    data,
                    responser: tx,
                };

                processor.send(req).await.unwrap();
                store.insert(si, rx).unwrap();
                Ok(None)
            } else {
                Ok(Some(Message::not_implemented("").set_si(si)))
            }
        } else {
            Ok(None)
        }
    }

    fn process_notice(&mut self, data: Data) -> Status {
        self.receive_notice_queue.push_front(data);
        Ok(())
    }

    async fn process_notice_next(
        data: Option<Data>,
        processor: &mut NoticeProcessor<Data>,
    ) -> Status {
        if let Some(data) = data {
            if let Some(processor) = processor {
                processor.send(data).await.unwrap();
            }
        }
        Ok(())
    }

    fn process_response(&mut self, si: SI, data: Result<Data>) -> Status {
        if let Some(waiter) = self.outgoing_requests.remove(&si) {
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

    fn send(&mut self, msg: Message<SI, Data>) -> Status {
        self.send_queue.push_front(msg);
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
