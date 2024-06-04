#[derive(Debug)]
pub enum ServerError {
    Hyper(hyper::Error),
    Noise(tokio_noise::NoiseError),
}

impl From<hyper::Error> for ServerError {
    fn from(e: hyper::Error) -> Self {
        ServerError::Hyper(e)
    }
}
impl From<tokio_noise::NoiseError> for ServerError {
    fn from(e: tokio_noise::NoiseError) -> Self {
        ServerError::Noise(e)
    }
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use ServerError::*;
        write!(
            f,
            "noise HTTP server error: {}",
            match self {
                Hyper(e) => format!("hyper error: {e}"),
                Noise(e) => format!("noise error: {e}"),
            }
        )
    }
}

impl std::error::Error for ServerError {}

#[derive(Debug)]
pub enum ClientError {
    Hyper(hyper::Error),
    Noise(tokio_noise::NoiseError),
}

impl From<hyper::Error> for ClientError {
    fn from(e: hyper::Error) -> Self {
        Self::Hyper(e)
    }
}
impl From<tokio_noise::NoiseError> for ClientError {
    fn from(e: tokio_noise::NoiseError) -> Self {
        Self::Noise(e)
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use ClientError::*;
        write!(
            f,
            "noise HTTP client error: {}",
            match self {
                Hyper(e) => format!("hyper error: {e}"),
                Noise(e) => format!("noise error: {e}"),
            }
        )
    }
}

impl std::error::Error for ClientError {}
