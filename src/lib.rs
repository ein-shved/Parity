// Simple request-response-notify bidirectional binary protocol implementation
// based on async

pub mod bidirect;
pub mod streams;
pub mod utils;

use std::future::Future;
use utils::*;

pub trait BidirectStream<'b, Data: 'b> {
    fn get_request_sender(&mut self) -> impl RequestSender<Data> + 'b;
    fn get_notice_sender(&mut self) -> impl NoticeSender<Data> + 'b;
    fn get_request_processor(&mut self) -> impl RequestProcessor<Data> + 'b;
    fn get_notice_processor(&mut self) -> impl NoticeProcessor<Data> + 'b;
    fn get_aborter(&mut self) -> impl Aborter + 'b;
}

pub trait RequestSender<Data> {
    fn request(&mut self, req: Data) -> impl Future<Output = Result<Data>> + Send;
}

pub trait NoticeSender<Data> {
    fn notify(&mut self, notice: Data) -> impl Future<Output = Status> + Send;
}

pub trait Request<Data> {
    fn get_data(&self) -> &Data;
    fn response(self, response: Result<Data>) -> impl Future<Output = Status> + Send;
}

pub trait RequestProcessor<Data> {
    fn next_request(&mut self) -> impl Future<Output = Result<impl Request<Data>>> + Send;
}

pub trait NoticeProcessor<Data> {
    fn next_notice(&mut self) -> impl Future<Output = Result<Data>> + Send;
}

pub trait Aborter {
    fn abort(&mut self) -> impl Future<Output = Status> + Send;
}
