use super::*;
use crate::streams::*;

use std::{collections::BTreeMap, io};

use tokio::{
    select,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

struct SelfRequest<Data> {
    pub msg: Data,
    pub tx: oneshot::Sender<Result<Data>>,
}

type SendQueue<Data, SI> = mpsc::Sender<Message<Data, SI>>;
type OutRequestsMap<Data, SI> = BTreeMap<SI, oneshot::Sender<Result<Data>>>;
type RequestProcessor<Data> = Option<mpsc::Sender<RequestImp<Data>>>;
type NoticeProcessor<Data> = Option<mpsc::Sender<Data>>;
type JobsSet = std::collections::LinkedList<JoinHandle<Status>>;

pub struct Bidirect<Data, SI = u16>
where
    SI: SeqId,
{
    seq_id: SI,

    request_sender: mpsc::Receiver<SelfRequest<Data>>,
    notice_sender: mpsc::Receiver<Data>,
    canceller: CancellationToken,

    request_processor: RequestProcessor<Data>,
    notice_processor: NoticeProcessor<Data>,

    request_sender_user: mpsc::Sender<SelfRequest<Data>>,
    notice_sender_user: mpsc::Sender<Data>,

    outgoing_requests: OutRequestsMap<Data, SI>,
}

pub struct DefaultRequestProcessor {}

impl<Data, SI> Bidirect<Data, SI>
where
    Data: 'static + Send,
    SI: SeqId + 'static + Send,
{
    pub fn new() -> Self {
        let (request_sender_user, request_sender) = mpsc::channel(16);
        let (notice_sender_user, notice_sender) = mpsc::channel(16);
        Self {
            seq_id: SI::zero(),

            request_sender,
            notice_sender,
            canceller: Default::default(),

            request_processor: Default::default(),
            notice_processor: Default::default(),

            request_sender_user,
            notice_sender_user,

            outgoing_requests: Default::default(),
        }
    }

    pub async fn event_loop(
        &mut self,
        mut sender: impl MessageSender<Data, SI> + 'static + Send,
        mut receiver: impl MessageReceiver<Data, SI>,
    ) -> Status {
        let (mpsc_send_tx, mut mpsc_send_rx) = mpsc::channel::<Message<Data, SI>>(16);
        let mut jobs = JobsSet::new();
        let canceller = self.canceller.clone();

        jobs.push_back(tokio::spawn(async move {
            println!("Starting loop");
            let res = loop {
                select! {
                    msg = mpsc_send_rx.recv() =>
                        if let Some(msg) = msg {
                            sender.send(msg).await?;
                        },
                    _ = canceller.cancelled() => {
                        println!("Loop breaked!");
                        break Result::Err(Error::new(io::ErrorKind::Interrupted, "Aborted by user"))
                    },
                }
            };
            println!("Loop finished");
            res
        }));

        let res = loop {
            select! {
                msg = receiver.recv() => self.process_next(msg, mpsc_send_tx.clone(), &mut jobs),

                req = self.request_sender.recv() => Self::send_request_next(
                        req,
                        &mut self.seq_id,
                        mpsc_send_tx.clone(),
                        &mut self.outgoing_requests, &mut jobs),

                not = self.notice_sender.recv() => Self::send_notice_next(
                        not,
                        mpsc_send_tx.clone(), &mut jobs),
                _ = self.canceller.cancelled() => {
                    break Result::Err(Error::new(io::ErrorKind::Interrupted, "Aborted by user"))
                },
            }?;
            let res = loop {
                if let Some(last) = jobs.back() {
                    if last.is_finished() {
                        let res = jobs.pop_back().unwrap().await?;
                        if res.is_err() {
                            break res;
                        }
                    } else {
                        break Ok(());
                    }
                } else {
                    break Result::Err(Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Jobs list is empty",
                    ));
                }
            };
            if res.is_err() {
                break res;
            }
        };

        while !jobs.is_empty() {
            let res = jobs.pop_back().unwrap().await?;
            if let Err(err) = res {
                println!("Error occured with job while aborting: {}", err);
            }
        }
        res
    }

    fn process_next(
        &mut self,
        msg: ResultMessage<Data, SI>,
        send: SendQueue<Data, SI>,
        jobs: &mut JobsSet,
    ) -> Status {
        match msg {
            Ok(msg) => match msg {
                Message::Request(si, data) => self.process_request((si, data), send.clone(), jobs),
                Message::Response(si, data) => self.process_response(si, Ok(data), jobs),
                Message::Notice(data) => self.process_notice(data, jobs),
                Message::Err(err, si) => self.process_err(err, si, jobs),
            },
            Err(err) => Err(err),
        }
    }

    fn process_request(
        &self,
        req: (SI, Data),
        sender: SendQueue<Data, SI>,
        jobs: &mut JobsSet,
    ) -> Status {
        let processor = self.request_processor.clone();
        jobs.push_back(tokio::spawn(async move {
            let (si, data) = req;
            let rsp = if let Some(processor) = processor {
                let (tx, rx) = oneshot::channel::<Result<Data>>();
                let req = RequestImp::<Data> {
                    data,
                    responser: tx,
                };

                processor.send(req).await.unwrap();
                let rsp = rx.await.unwrap();
                match rsp {
                    Ok(rsp) => Message::Response(si, rsp),
                    Err(err) => Message::Err(err, Some(si)),
                }
            } else {
                Message::not_implemented("").set_si(si)
            };
            sender.send(rsp).await?;
            Ok(()) as Status
        }));
        Ok(())
    }

    fn process_notice(&self, data: Data, jobs: &mut JobsSet) -> Status {
        let processor = self.notice_processor.clone();
        jobs.push_back(tokio::spawn(async move {
            if let Some(processor) = processor {
                processor.send(data).await?;
            }
            Ok(()) as Status
        }));
        Ok(())
    }

    fn process_response(&mut self, si: SI, data: Result<Data>, _: &mut JobsSet) -> Status {
        if let Some(waiter) = self.outgoing_requests.remove(&si) {
            waiter
                .send(data)
                .map_err(|_| Error::pipe(&format!("Response waiter closed for {}", si)))?;
        }
        Ok(())
    }

    fn process_err(&mut self, data: Error, si: Option<SI>, jobs: &mut JobsSet) -> Status {
        if let Some(si) = si {
            self.process_response(si, Err(data), jobs)
        } else {
            // TODO(Shvedov): Notify user about error
            Ok(())
        }
    }

    fn send_request_next(
        req: Option<SelfRequest<Data>>,
        si: &mut SI,
        sender: SendQueue<Data, SI>,
        requests_map: &mut OutRequestsMap<Data, SI>,
        jobs: &mut JobsSet,
    ) -> Status {
        if let Some(req) = req {
            let mysi = *si;
            *si = (*si).inc();
            requests_map.insert(mysi, req.tx);
            jobs.push_back(tokio::spawn(async move {
                sender.send(Message::Request(mysi, req.msg)).await?;
                Ok(()) as Status
            }));
        }
        Ok(())
    }

    fn send_notice_next(
        not: Option<Data>,
        sender: SendQueue<Data, SI>,
        jobs: &mut JobsSet,
    ) -> Status {
        if let Some(not) = not {
            jobs.push_back(tokio::spawn(async move {
                sender.send(Message::Notice(not)).await?;
                Ok(()) as Status
            }));
        }
        Ok(())
    }
}

struct RequestImp<Data> {
    data: Data,
    responser: oneshot::Sender<Result<Data>>,
}

#[async_trait]
impl<Data: Send> Request<Data> for RequestImp<Data> {
    fn get_data(&self) -> &Data {
        &self.data
    }
    async fn response(self, response: Result<Data>) -> Status {
        self.responser
            .send(response)
            .map_err(|_| Error::pipe("Failed to pipe response to requester"))?;
        Ok(())
    }
}

struct RequestSenderImpl<Data> {
    channel: mpsc::Sender<SelfRequest<Data>>,
}

#[async_trait]
impl<Data: Send> RequestSender<Data> for RequestSenderImpl<Data> {
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

struct NoticeSenderImpl<Data> {
    channel: mpsc::Sender<Data>,
}

#[async_trait]
impl<Data: Send> NoticeSender<Data> for NoticeSenderImpl<Data> {
    async fn notify(&mut self, notice: Data) -> Status {
        self.channel.send(notice).await.unwrap();
        Ok(())
    }
}

#[tokio::test]
async fn get_notice_sender() {
    let mut bidir = Bidirect::<String>::new();
    let mut sender = bidir.get_notice_sender();
    _ = sender.notify(String::from("Hello world!"));
}

struct RequestProcessorImpl<Data> {
    channel: mpsc::Receiver<RequestImp<Data>>,
}

#[async_trait]
impl<Data: Send> crate::RequestProcessor<Data> for RequestProcessorImpl<Data> {
    async fn next_request(&mut self) -> Result<RequestImp<Data>> {
        let req = self.channel.recv().await.unwrap();
        Ok(req)
    }
}

#[tokio::test]
async fn get_request_processor() {
    use crate::RequestProcessor;

    let mut bidir = Bidirect::<String>::new();
    let mut processor = bidir.get_request_processor();
    _ = processor.next_request();
}

struct NoticeProcessorImpl<Data> {
    channel: mpsc::Receiver<Data>,
}

#[async_trait]
impl<Data: Send> crate::NoticeProcessor<Data> for NoticeProcessorImpl<Data> {
    async fn next_notice(&mut self) -> Result<Data> {
        let req = self.channel.recv().await.unwrap();
        Ok(req)
    }
}

#[tokio::test]
async fn get_notice_processor() {
    use crate::NoticeProcessor;

    let mut bidir = Bidirect::<String>::new();
    let mut processor = bidir.get_notice_processor();
    _ = processor.next_notice();
}

struct AborterImpl {
    canceller: CancellationToken,
}

#[async_trait]
impl crate::Aborter for AborterImpl {
    async fn abort(&mut self) -> Status {
        self.canceller.cancel();
        Ok(())
    }
}

#[tokio::test]
async fn get_aborter() {
    let mut bidir = Bidirect::<String>::new();
    let mut aborter = bidir.get_aborter();
    _ = aborter.abort();
}

impl<'b, Data: 'b + Send, SI: SeqId> BidirectStream<'b, Data> for Bidirect<Data, SI> {
    fn get_request_sender(&mut self) -> impl RequestSender<Data> + 'b {
        RequestSenderImpl {
            channel: self.request_sender_user.clone(),
        }
    }

    fn get_notice_sender(&mut self) -> impl NoticeSender<Data> + 'b {
        NoticeSenderImpl {
            channel: self.notice_sender_user.clone(),
        }
    }

    fn get_request_processor(&mut self) -> impl crate::RequestProcessor<Data> + 'b {
        let (tx, rx) = mpsc::channel(16);
        self.request_processor = Some(tx);
        RequestProcessorImpl { channel: rx }
    }

    fn get_notice_processor(&mut self) -> impl crate::NoticeProcessor<Data> + 'b {
        let (tx, rx) = mpsc::channel(16);
        self.notice_processor = Some(tx);
        NoticeProcessorImpl { channel: rx }
    }

    fn get_aborter(&mut self) -> impl crate::Aborter + 'b {
        AborterImpl {
            canceller: self.canceller.clone(),
        }
    }
}
