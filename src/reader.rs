use bytes::{Bytes, BytesMut};
use futures::io::{AsyncRead, AsyncSeek};
use futures::Future;
// use futures::FutureExt;
use reqwest::Client;
// use std::future::IntoFuture;
// use std::future::Future;
use std::io::{self, SeekFrom};
use std::pin::Pin;
// use std::process::Output;
use std::task::{Context, Poll};
use worker as cf;

/**
* A buffered reader that fetches data in pages, increasing the page size for each fetch.

*/
// https://blog.cloudflare.com/pin-and-unpin-in-rust
#[pin_project::pin_project]
pub struct BufferedReader {
    url: String,
    buffer: BytesMut,
    position: u64,
    total_size: u64,
    #[pin]
    future: Box<dyn Future<Output = io::Result<Bytes>>>,

    // Settings
    range_size: usize,
    range_size_multiplier: usize,
    max_range_size: usize,
}

impl BufferedReader {
    pub async fn new(
        url: String,
        initial_page_size: Option<usize>,
        max_page_size: Option<usize>,
        page_size_multiple: Option<usize>,
    ) -> io::Result<Self> {
        // TODO: Do we actually need to know this information before reading the data?
        let client = Client::new();
        cf::console_log!("Fetching content length for {}", &url);
        let response = client
            .head(&url)
            .send()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let total_size = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|len| len.to_str().ok())
            .and_then(|len| len.parse().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to get content length"))?;
        cf::console_log!("Content length: {}", total_size);

        let current_page_size = initial_page_size.unwrap_or(4096);
        let fetch = Self::_fetch_page(url.clone(), 0, current_page_size as u64);

        Ok(Self {
            url,
            buffer: BytesMut::new(),
            position: 0,
            range_size: current_page_size,
            total_size,
            future: Box::new(fetch),

            max_range_size: max_page_size.unwrap_or(4_194_304),
            range_size_multiplier: page_size_multiple.unwrap_or(2),
        })
    }

    async fn fetch_page(mut self) -> io::Result<Bytes> {
        let start = self.position;
        let end = start + self.range_size as u64;
        let response = Self::_fetch_page(self.url, start, end).await?;
        self.range_size = (self.range_size * self.range_size_multiplier).min(self.max_range_size);
        Ok(response)
    }
    async fn _fetch_page(url: String, start: u64, end: u64) -> io::Result<Bytes> {
        let client = Client::new();
        cf::console_log!("Fetching page: {}-{}", start, end);
        let range_header = format!("bytes={}-{}", start, end - 1);

        let response = client
            .get(url)
            .header(reqwest::header::RANGE, range_header)
            .send()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let data = response
            .bytes()
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(data)
    }

    fn increment_page_size(&mut self) {}
}

impl AsyncRead for BufferedReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let mut this = self.project();

        if this.buffer.is_empty() {
            // match this.future.as_mut().poll(cx) {
            //     Poll::Pending => return Poll::Pending,
            //     Poll::Ready(Ok(data)) => {
            //         // Append the fetched data to the buffer
            //         this.buffer.extend_from_slice(&data);
            //         // Setup the next request for polling
            //         // this.future = Box::new(this.fetch_page());
            //     }
            // };
        }

        let len = std::cmp::min(buf.len(), this.buffer.len());
        buf[..len].copy_from_slice(&this.buffer.split_to(len));

        *this.position += len as u64;

        Poll::Ready(Ok(len))
    }
}

// impl AsyncSeek for BufferedReader {
//     fn poll_seek(
//         mut self: Pin<&mut Self>,
//         _cx: &mut Context<'_>,
//         pos: SeekFrom,
//     ) -> Poll<io::Result<u64>> {
//         let new_position = match pos {
//             SeekFrom::Start(offset) => offset,
//             SeekFrom::End(offset) => {
//                 if offset < 0 {
//                     self.total_size.saturating_sub(offset.abs() as u64)
//                 } else {
//                     self.total_size.saturating_add(offset as u64)
//                 }
//             }
//             SeekFrom::Current(offset) => {
//                 if offset < 0 {
//                     self.position.saturating_sub(offset.abs() as u64)
//                 } else {
//                     self.position.saturating_add(offset as u64)
//                 }
//             }
//         };

//         self.position = new_position;
//         self.buffer.clear(); // Clear the buffer to refetch data
//         self.future = None; // Reset the fetch future
//     }
// }
