use futures::io::{AsyncRead, AsyncSeek};
use futures::task::Poll;
use futures::{future, FutureExt, TryFutureExt};
use http_range_client::{BufferedHttpRangeClient, HttpError};
use std::pin::{pin, Pin};

#[pin_project::pin_project]
pub struct Reader {
    pub client: BufferedHttpRangeClient,
    #[pin]
    future: Option<future::BoxFuture<'static, Result<Vec<u8>, HttpError>>>,
}

impl AsyncRead for Reader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut futures::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize, futures::io::Error>> {
        let mut this = self.project();
        if this.future.is_none() {
            this.future.set(Some(
                this.client
                    .get_bytes(buf.len())
                    .map_ok(|res| res.to_vec())
                    .boxed::<'static>()
            ));
        }

        match this.future.unwrap().as_mut().poll(cx) {
            Poll::Ready(Ok(bytes)) => {
                let len = bytes.len();
                buf[..len].copy_from_slice(&bytes);
                Poll::Ready(Ok(len))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(futures::io::Error::new(
                futures::io::ErrorKind::Other,
                e.to_string(),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Reader {
    pub fn new(url: &String) -> Self {
        Reader {
            client: BufferedHttpRangeClient::new(&url),
            future: None,
        }
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
