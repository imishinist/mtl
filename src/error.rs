use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("invalid format")]
    InvalidFormat,

    #[error("invalid token")]
    InvalidToken(String),

    #[error("io error")]
    IOError(#[from] io::Error),
}
