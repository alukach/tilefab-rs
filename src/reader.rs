use futures::io::AsyncRead;
use futures::task::Poll;
use futures::{pin_mut, Future, FutureExt, TryFutureExt};
use http_range_client::{BufferedHttpRangeClient, HttpError};
use std::pin::{pin, Pin};

#[pin_project::pin_project]
pub struct Reader {
    pub client: BufferedHttpRangeClient,
    pub min_request_size: usize,
    pub min_request_size_factor: usize,
    pub min_request_size_max: usize,
    #[pin]
    fut: Option<Box<dyn Future<Output = Result<Vec<u8>, HttpError>>>>,
}

impl Reader {
    pub fn new(url: &String) -> Self {
        Reader {
            client: BufferedHttpRangeClient::new(&url),
            min_request_size: 4096,
            min_request_size_factor: 2,
            min_request_size_max: 4096 * 1024,
            fut: None,
        }
    }
}

impl AsyncRead for Reader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut futures::task::Context<'_>,
        buf: &mut [u8],
    ) -> futures::task::Poll<Result<usize, futures::io::Error>> {
        let this = self.project();

        // TODO: Consider incrementing page side after each read
        // this.client.set_min_req_size(
        //     (this.min_request_size * this.min_request_size_factor).min(this.min_request_size_max),
        // );

        if this.fut.is_none() {
            let operation = this.client.get_bytes(buf.len()).map_ok(|res| res.to_vec());
            this.fut = pin!(Some(Box::new(operation)));
        }

        this.fut.as_ref().expect("msg").as_mut().poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::AsyncReadExt;

    #[tokio::test]
    async fn test_integer_generator_initial_value() {
        let mut reader = Reader::new(&String::from("https://example.com"));

        let mut output = [0; 25];
        let bytes = reader.read(&mut output[..]).await.unwrap();

        println!("bytes v1: {:?}, data: {:?}", bytes, &output[..bytes]);
        // println!("The position: {:?}", &reader.current);
        let mut output = [0; 22];

        let bytes = reader.read(&mut output[..]).await.unwrap();
        println!("bytes v2: {:?}, data: {:?}", bytes, &output[..bytes]);
        // println!("The position: {:?}", &reader.current);
        // assert_eq!(reader.pages, 2);
    }
}
