use std::io;
use std::num::ParseIntError;

#[derive(thiserror::Error, Debug)]
pub enum ReadContentError {
    #[error(transparent)]
    IOError(#[from] io::Error),

    #[error(transparent)]
    ParseError(#[from] ParseError),
}

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("empty token")]
    EmptyToken,

    #[error("invalid token: \"{0}\"")]
    InvalidToken(String),

    #[error("invalid hash: {0}")]
    Hash(#[from] ParseHashError),
}

#[derive(thiserror::Error, Debug)]
pub enum ParseHashError {
    #[error("invalid hash format")]
    InvalidFormat,

    #[error("invalid hash token: \"{0}\"")]
    InvalidToken(String),

    #[error(transparent)]
    IntError(#[from] ParseIntError),
}
