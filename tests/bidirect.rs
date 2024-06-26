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

    pub fn connect_to(&mut self, other: &mut Self) {
        let tx = self.tx();
        let mut rx = other.rx().unwrap();
        self.add(async move {
            loop {
                let msg = rx.recv().await;
                if let Some(msg) = msg {
                    tx.send(msg).await.unwrap();
                } else {
                    break;
                }
            }
        })
    }

    pub async fn run_double(mut self, mut other: Self) {
        self.connect_to(&mut other);
        other.connect_to(&mut self);

        self.add(other.run());

        self.run().await
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

#[tokio::test]
async fn test_send_response() {
    let mut test = <TestMaker>::make_test_set();

    let mut proc = test.stream().get_request_processor();
    let mut abort = test.stream().get_aborter();

    let tx = test.tx();
    let mut rx = test.rx().unwrap();

    test.add(async move {
        tx.send(Message::Request(10, "Hello world?".into()))
            .await
            .unwrap();
        let rsp = rx.recv().await.unwrap();
        if let Message::Response(si, data) = rsp {
            assert!(si == 10);
            assert!(data == "Hello world!");
            println!("Got response: {}", data);
        } else {
            panic!("Message is not response");
        }
        abort.abort().await.unwrap();
    });

    test.add(async move {
        let req = proc.next_request().await.unwrap();
        let req_data = req.get_data();

        assert!(req_data == "Hello world?");
        req.response(Ok("Hello world!".into())).await.unwrap();
    });

    test.run().await;
}

#[tokio::test]
async fn test_recv_notice() {
    let mut test = <TestMaker>::make_test_set();

    let mut notc = test.stream().get_notice_processor();
    let mut abort = test.stream().get_aborter();

    let tx = test.tx();

    test.add(async move {
        tx.send(Message::Notice("Hello world?".into()))
            .await
            .unwrap();
    });

    test.add(async move {
        let data = notc.next_notice().await.unwrap();

        println!("Got notice: {}", data);
        assert!(data == "Hello world?");

        abort.abort().await.unwrap();
    });

    test.run().await;
}

#[tokio::test]
async fn test_send_notice() {
    let mut test = <TestMaker>::make_test_set();

    let mut notc = test.stream().get_notice_sender();
    let mut abort = test.stream().get_aborter();

    let mut rx = test.rx().unwrap();

    test.add(async move {
        let data = rx.recv().await.unwrap();
        if let Message::Notice(data) = data {
            assert!(data == "Hello world!");
            println!("Got notice: {}", data);
        } else {
            panic!("Message is not notice");
        }

        abort.abort().await.unwrap();
    });

    test.add(async move {
        notc.notify("Hello world!".into()).await.unwrap();
    });

    test.run().await;
}

#[tokio::test]
async fn test_double_request()
{
    let mut t1 = <TestMaker>::make_test_set();
    let mut t2 = <TestMaker>::make_test_set();

    let mut req = t1.stream().get_request_sender();
    let mut proc = t2.stream().get_request_processor();

    let mut abort1 = t1.stream().get_aborter();
    let mut abort2 = t2.stream().get_aborter();

    t1.add(async move {
        let res = req.request("Hello world?".into()).await.unwrap();
        println!("Got responce: {}", res);
        assert!(res == "Hello world!");

        abort1.abort().await.unwrap();
        abort2.abort().await.unwrap();
    });

    t2.add(async move {
        let req = proc.next_request().await.unwrap();
        let req_data = req.get_data();

        println!("Got request: {}", req_data);
        assert!(req_data == "Hello world?");
        req.response(Ok("Hello world!".into())).await.unwrap();
    });

    t1.run_double(t2).await;
}

#[tokio::test]
async fn test_double_notice()
{
    let mut t1 = <TestMaker>::make_test_set();
    let mut t2 = <TestMaker>::make_test_set();

    let mut ntc1 = t1.stream().get_notice_sender();
    let mut ntc2 = t2.stream().get_notice_processor();

    let mut abort1 = t1.stream().get_aborter();
    let mut abort2 = t2.stream().get_aborter();

    t1.add(async move {
        ntc1.notify("Hello world!".into()).await.unwrap();
    });

    t2.add(async move {
        let data = ntc2.next_notice().await.unwrap();

        println!("Got notice: {}", data);
        assert!(data == "Hello world!");

        abort1.abort().await.unwrap();
        abort2.abort().await.unwrap();
    });

    t1.run_double(t2).await;
}


#[tokio::test]
async fn test_send_multi_request() {
    let mut test = <TestMaker>::make_test_set();

    let mut req1 = test.stream().get_request_sender();
    let mut req2 = test.stream().get_request_sender();
    let mut req3 = test.stream().get_request_sender();

    let mut abort = test.stream().get_aborter();
    let tx = test.tx();
    let mut rx = test.rx().unwrap();

    let (abort_tx1, mut abort_rx) = mpsc::channel::<()>(3);
    let abort_tx2 = abort_tx1.clone();
    let abort_tx3 = abort_tx1.clone();

    test.add(async move {
        let res = req1.request("Hello world 1".into()).await.unwrap();
        println!("Got responce: {}", res);
        assert!(res == "Hello world 1 + Hello!");
        abort_tx1.send(()).await.unwrap();
    });

    test.add(async move {
        let res = req2.request("Hello world 2".into()).await.unwrap();
        println!("Got responce: {}", res);
        assert!(res == "Hello world 2 + Hello!");
        abort_tx2.send(()).await.unwrap();
    });

    test.add(async move {
        let res = req3.request("Hello world 3".into()).await.unwrap();
        println!("Got responce: {}", res);
        assert!(res == "Hello world 3 + Hello!");
        abort_tx3.send(()).await.unwrap();
    });

    test.add(async move {
        abort_rx.recv().await.unwrap();
        abort_rx.recv().await.unwrap();
        abort_rx.recv().await.unwrap();

        abort.abort().await.unwrap();
    });

    test.add(async move {
        let msg1 = rx.recv().await.unwrap();
        let msg2 = rx.recv().await.unwrap();
        let msg3 = rx.recv().await.unwrap();

        async fn check_send_response(msg: Message<String, u16>, tx: &mpsc::Sender<Message<String, u16>>)
        {
            if let Message::Request(si, data) = msg {
                assert!(&data[0 .. 12] == "Hello world ");
                println!("Got request: {}", data);
                tx.send(Message::Response(si, data + " + Hello!"))
                    .await
                    .unwrap();
            } else {
                panic!("Message is not Request!");
            }
        }
        check_send_response(msg1, &tx).await;
        check_send_response(msg2, &tx).await;
        check_send_response(msg3, &tx).await;
    });

    test.run().await;
}
