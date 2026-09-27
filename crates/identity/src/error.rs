//! Errors produced by the identity layer.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid public key: {0}")]
    PublicKey(String),
    #[error("invalid signature: {0}")]
    Signature(String),
    #[error("invalid hex encoding: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("signature verification failed")]
    Verification,
    #[error("system RNG error: {0}")]
    Random(String),
    #[error("invalid key file: {0}")]
    KeyFile(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
