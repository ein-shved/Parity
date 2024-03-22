// Simple request-response-notify bidirectional binary protocol implementation
// based on async

pub mod streams;
pub mod utils;
pub mod bidirect;

use utils::*;

pub trait BidirectStream<'b, Data: 'b> {
    fn get_request_sender(&mut self) -> impl RequestSender<Data> + 'b;
    fn get_notice_sender(&mut self) -> impl NoticeSender<Data> + 'b;
    fn get_request_processor(&mut self) -> impl RequestProcessor<Data> + 'b;
    fn get_notice_processor(&mut self) -> impl NoticeProcessor<Data> + 'b;
    fn get_aborter(&mut self) -> impl Aborter + 'b;
}

pub trait RequestSender<Data> {
    async fn request(&mut self, req: Data) -> Result<Data>;
}

pub trait NoticeSender<Data> {
    async fn notify(&mut self, notice: Data) -> Status;
}

pub trait Request<Data> {
    fn get_data(&self) -> &Data;
    async fn response(self, response: Result<Data>) -> Status;
}

pub trait RequestProcessor<Data> {
    async fn next_request(&mut self) -> Result<impl Request<Data>>;
}

pub trait NoticeProcessor<Data> {
    async fn next_notice(&mut self) -> Result<Data>;
}

pub trait Aborter {
    async fn abort(&mut self) -> Status;
}
