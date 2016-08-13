extern crate glob;
use std::io;
use zip::result::ZipError;
use hyper;
use std::fmt;

#[derive(Debug)]
pub enum SubError {
    NetworkError(hyper::error::Error),
    Io(io::Error),
    Pattern(glob::PatternError),
    Zip(ZipError),
    ZipEmpty,
    SvrInvalidResponse,
    SvrInvalidCredentials,
    SvrNoSubtitlesFound,
}

// Implement `Display` for `MinMax`.
impl fmt::Display for SubError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SubError::NetworkError(ref err) => write!(f, "{:?}", err),
            SubError::Io(ref err) => write!(f, "{:?}", err),
            SubError::Pattern(ref err) => write!(f, "{:?}", err),
            SubError::Zip(ref err) => write!(f, "{:?}", err),
            SubError::ZipEmpty => write!(f, "Zip file empty"),
            SubError::SvrInvalidResponse => write!(f, "SvrInvalidResponse"),
            SubError::SvrInvalidCredentials => write!(f, "SvrInvalidCredentials"),
            SubError::SvrNoSubtitlesFound => write!(f, "SvrNoSubtitlesFound"),
        }
    }
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

impl From<glob::PatternError> for SubError {
    fn from(err: glob::PatternError) -> SubError {
        SubError::Pattern(err)
    }
}

impl From<ZipError> for SubError {
    fn from(err: ZipError) -> SubError {
        SubError::Zip(err)
    }
}
