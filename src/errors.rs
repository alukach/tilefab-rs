pub enum CogErr {
    HttpError(http_range_client::HttpError),
    IoError(std::io::Error),
}

impl From<std::io::Error> for CogErr {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<http_range_client::HttpError> for CogErr {
    fn from(e: http_range_client::HttpError) -> Self {
        Self::HttpError(e)
    }
}

impl std::fmt::Display for CogErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The `f` value implements the `Write` trait, which is what the
        // write! macro is expecting. Note that this formatting ignores the
        // various flags provided to format strings.
        write!(f, "ERR")
    }
}
