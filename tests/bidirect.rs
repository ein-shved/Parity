use parity::bidirect::*;
use parity::streams::*;
use parity::utils::*;
use parity::*;

use core::future::Future;
use std::marker::PhantomData;
use tokio::{sync::mpsc, task};

struct MpscSender<Data> {
    sender: mpsc::Sender<Data>,
}

type MpscMsgSender<Data = String, SI = u16> = MpscSender<Message<Data, SI>>;

impl<Data> Sender<Data> for MpscSender<Data> {
    async fn send(&mut self, msg: Data) -> Status {
        self.sender.send(msg).await.unwrap();
        Ok(())
    }
}

struct MpscReceiver<Data> {
    receiver: mpsc::Receiver<Data>,
}
type MpscMsgReceiver<Data = String, SI = u16> = MpscReceiver<Message<Data, SI>>;

impl<Data> Receiver<Data> for MpscReceiver<Data> {
    async fn recv(&mut self) -> Result<Data> {
        Ok(self.receiver.recv().await.unwrap())
    }
}

struct TestMaker<Data = String, SI = u16> {
    ph1: PhantomData<Data>,
    ph2: PhantomData<SI>,
}

impl<'a, Data: 'a, SI: SeqId + 'a> TestMaker<Data, SI> {
    pub fn make_test_sender() -> (impl Sender<Data>, mpsc::Receiver<Data>) {
        let (tx, rx) = mpsc::channel(16);
        (MpscSender { sender: tx }, rx)
    }

    pub fn make_test_receiver() -> (mpsc::Sender<Data>, impl Receiver<Data>) {
        let (tx, rx) = mpsc::channel(16);
        (tx, MpscReceiver { receiver: rx })
    }

    pub fn make_test_set(
    ) -> TestSet<impl MessageSender<Data, SI>, impl MessageReceiver<Data, SI>, Data, SI> {
        let (mtx, rx) = TestMaker::<Message<Data, SI>>::make_test_sender();
        let (tx, mrx) = TestMaker::<Message<Data, SI>>::make_test_receiver();
        let stream = Bidirect::<'static, Data, SI>::new();

        let local = task::LocalSet::new();

        TestSet {
            stream,
            mtx,
            mrx,
            tx,
            rx: Some(rx),
            local,
            tasks: Default::default(),
        }
    }
}

struct TestSet<
    Snd: MessageSender<Data, SI>,
    Rcv: MessageReceiver<Data, SI>,
    Data: 'static = String,
    SI: SeqId + 'static = u16,
> {
    stream: Bidirect<'static, Data, SI>,
    mtx: Snd,
    mrx: Rcv,
    tx: mpsc::Sender<Message<Data, SI>>,
    rx: Option<mpsc::Receiver<Message<Data, SI>>>,

    local: task::LocalSet,
    tasks: std::vec::Vec<tokio::task::JoinHandle<()>>,
}

impl<
        Snd: MessageSender<Data, SI> + 'static,
        Rcv: MessageReceiver<Data, SI> + 'static,
        Data: 'static,
        SI: SeqId + 'static,
    > TestSet<Snd, Rcv, Data, SI>
{
    pub fn stream(&mut self) -> &mut Bidirect<'static, Data, SI> {
        &mut self.stream
    }

    pub fn tx(&self) -> mpsc::Sender<Message<Data, SI>> {
        self.tx.clone()
    }

    pub fn rx(&mut self) -> Option<mpsc::Receiver<Message<Data, SI>>> {
        let mut res = None;
        std::mem::swap(&mut self.rx, &mut res);
        res
    }

    pub async fn looper(mut self) {
        loop {
            let res = &mut self.stream.next(&mut self.mtx, &mut self.mrx).await;
            if let Err(err) = res {
                println!("Stream finished with: {}", err);
                break;
            }
        }
    }

    pub async fn run(mut self) {
        let mut local = Default::default();
        let mut tasks = Default::default();

        std::mem::swap(&mut self.local, &mut local);
        std::mem::swap(&mut self.tasks, &mut tasks);

        let main = local.spawn_local(self.looper());

        local.await;

        main.await.unwrap();
        for test in tasks {
            test.await.unwrap();
        }
    }

    pub fn add<F>(&mut self, f: F)
    where
        F: Future<Output = ()> + 'static,
    {
        let task = self.local.spawn_local(f);
        self.tasks.push(task)
    }
}

#[tokio::test]
async fn test_abort() {
    let mut test = <TestMaker>::make_test_set();
    let mut abort = test.stream().get_aborter();

    test.add(async move {
        abort.abort().await.unwrap();
    });

    test.run().await;
}

#[tokio::test]
async fn test_send_request() {
    let mut test = <TestMaker>::make_test_set();

    let mut req = test.stream().get_request_sender();
    let mut abort = test.stream().get_aborter();
    let tx = test.tx();
    let mut rx = test.rx().unwrap();

    test.add(async move {
        let res = req.request("Hello world?".into()).await.unwrap();
        println!("Got responce: {}", res);
        assert!(res == "Hello world!");
        abort.abort().await.unwrap();
    });

    test.add(async move {
        let msg = rx.recv().await.unwrap();
        if let Message::Request(si, data) = msg {
            assert!(si == 0);
            assert!(data == "Hello world?");
            println!("Got request: {}", data);
        } else {
            panic!("Message is not Request!");
        }
        tx.send(Message::Response(0, "Hello world!".into()))
            .await
            .unwrap();
    });

    test.run().await;
}
