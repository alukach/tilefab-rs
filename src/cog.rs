use reqwest::{
    header::{HeaderMap, RANGE},
    Error,
};

pub struct COG {
    pub src: String,
}

impl COG {
    pub async fn fetch_header(&self) -> Result<String, Error> {
        // Fetch tile header
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert(RANGE, "bytes=0-1023".parse().unwrap());

        // Make the GET request with the headers
        let response = client.get(&self.src).headers(headers).send().await?;

        // Print the status and the response body (if any)
        println!("Status: {}", response.status());
        response.text().await
    }
}
