use std::io;
use std::num::ParseIntError;

use redb::{StorageError, TableError, TransactionError};

#[derive(thiserror::Error, Debug)]
pub enum ReadContentError {
    #[error("object not found")]
    ObjectNotFound,

    #[error(transparent)]
    IOError(#[from] io::Error),

    #[error(transparent)]
    ParseError(#[from] ParseError),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    StorageError(#[from] StorageError),

    #[error(transparent)]
    TableError(#[from] TableError),

    #[error(transparent)]
    TransactionError(#[from] TransactionError),
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
