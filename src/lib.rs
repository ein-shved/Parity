pub mod streams;
pub mod utils;
pub mod bidirect;

use utils::*;

pub trait BidirectStream<Data, Req: Request<Data>> {
    fn get_request_sender(&mut self) -> impl RequestSender<Data>;
    fn get_notice_sender(&mut self) -> impl NoticeSender<Data>;
    fn get_request_processor(&mut self) -> impl RequestProcessor<Data, Req>;
    fn get_notice_processor(&mut self) -> impl NoticeProcessor<Data>;
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

pub trait RequestProcessor<Data, Req: Request<Data>> {
    async fn next_request(&mut self) -> Result<Req>;
}

pub trait NoticeProcessor<Data> {
    async fn next_notice(&mut self) -> Result<Data>;
}
