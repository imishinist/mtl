use std::io;
use std::num::ParseIntError;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("invalid format")]
    InvalidFormat,

    #[error("invalid token")]
    InvalidToken(String),

    #[error("parse int error")]
    IntError(#[from] ParseIntError),

    #[error("io error")]
    IOError(#[from] io::Error),
}
