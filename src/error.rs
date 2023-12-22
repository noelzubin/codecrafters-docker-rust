use std::io;
use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum ContainerError {
    #[error("HTTP: {0}")]
    Http(String),

    #[error("Unhandled status code: {0}")]
    UnhandledStatusCode(StatusCode),

    #[error("Auth error: {0}")]
    Auth(&'static str),

    #[error("Unsupported manifest file: {0}")]
    Manifest(&'static str),

    #[error("I/O error: {0}")]
    Io(String),
}

impl From<reqwest::Error> for ContainerError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err.to_string())
    }
}

impl From<io::Error> for ContainerError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}
