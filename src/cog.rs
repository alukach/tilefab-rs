use bytes::Bytes;

use reqwest::{
    header::{HeaderMap, RANGE},
    Error as ReqError,
};

pub struct COG {
    pub src: String,
}

impl COG {
    pub async fn fetch_header(&self, bytes: u32) -> Result<Bytes, ReqError> {
        // Fetch tile header
        let client = reqwest::Client::new();
        let range = format!("bytes=0-{}", bytes);
        let mut headers = HeaderMap::new();
        headers.insert(RANGE, range.parse().unwrap());

        // Make the GET request with the headers
        let response = client.get(&self.src).headers(headers).send().await?;

        // Parse header
        Ok(response.bytes().await?)
    }
}
