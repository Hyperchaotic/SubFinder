use std::io;
use zip::result::ZipError;
use hyper;

#[derive(Debug)]
pub enum SubError {
    NetworkError(hyper::error::Error),
    Io(io::Error),
    Zip(ZipError),
    ZipEmpty,
    SvrInvalidResponse,
    SvrInvalidCredentials,
    SvrNoSubtitlesFound,
}

impl From<hyper::error::Error> for SubError {
    fn from(err: hyper::error::Error) -> SubError {
        SubError::NetworkError(err)
    }
}

impl From<io::Error> for SubError {
    fn from(err: io::Error) -> SubError {
        SubError::Io(err)
    }
}

impl From<ZipError> for SubError {
    fn from(err: ZipError) -> SubError {
        SubError::Zip(err)
    }
}
