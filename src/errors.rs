#[derive(Debug)]

pub enum CogErr {
    Http(http_range_client::HttpError),
    Io(std::io::Error),
}

impl From<std::io::Error> for CogErr {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<http_range_client::HttpError> for CogErr {
    fn from(e: http_range_client::HttpError) -> Self {
        Self::Http(e)
    }
}

impl std::fmt::Display for CogErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            // Both underlying errors already impl `Display`, so we defer to
            // their implementations.
            // TODO: There must be a better way to do this
            Self::Io(ref err) => write!(f, "{{\"err\": {{\"kind\": \"IO Error\", \"details\": \"{}\"}}", err),
            Self::Http(ref err) => write!(f, "{{\"err\": {{\"kind\": \"HTTP Error\", \"details\": \"{}\"}}", err),
        }
    }
}
