use reqwest::{
    header::{HeaderMap, RANGE},
    Error as ReqError,
};

pub mod tiff_header;

pub struct COG {
    pub src: String,
}

impl COG {
    pub async fn fetch_header(&self) -> Result<Vec<u8>, ReqError> {
        // Fetch tile header
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(RANGE, "bytes=0-1023".parse().unwrap());

        // Make the GET request with the headers
        let response = client.get(&self.src).headers(headers).send().await?;

        // Parse header
        Ok(response.bytes().await?.to_vec())
    }
}
