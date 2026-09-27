//! Error types shared by the networking layer.

use thiserror::Error;

/// Errors produced while setting up or serving on the network.
#[derive(Debug, Error)]
pub enum NetError {
    #[error("transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] hyper::Error),
    #[error("identity error: {0}")]
    Identity(#[from] identity::Error),
    #[error("invalid address `{address}`: {reason}")]
    Address { address: String, reason: String },
    #[error("no reachable peers")]
    NoPeers,
}

pub type Result<T, E = NetError> = std::result::Result<T, E>;

/// Errors produced by a single peer call.
///
/// This is deliberately `Clone` so it can flow through `tower`'s retry and
/// load-balancing middleware.
#[derive(Debug, Clone, Error)]
pub enum PeerError {
    #[error("peer returned gRPC status {code:?}: {message}")]
    Status { code: tonic::Code, message: String },
    #[error("peer transport error: {0}")]
    Transport(String),
    #[error("peer request timed out")]
    Timeout,
    #[error("peer message was not authenticated: {0}")]
    Unauthenticated(String),
    #[error("failed to sign peer message: {0}")]
    Signing(String),
    #[error("unexpected response from peer")]
    UnexpectedResponse,
}

impl PeerError {
    /// Whether a request that failed with this error is worth retrying.
    pub fn is_retryable(&self) -> bool {
        match self {
            PeerError::Transport(_) | PeerError::Timeout => true,
            PeerError::Status { code, .. } => matches!(
                code,
                tonic::Code::Unavailable
                    | tonic::Code::Unknown
                    | tonic::Code::DeadlineExceeded
                    | tonic::Code::Aborted
                    | tonic::Code::ResourceExhausted
            ),
            PeerError::Unauthenticated(_)
            | PeerError::Signing(_)
            | PeerError::UnexpectedResponse => false,
        }
    }
}

impl From<tonic::Status> for PeerError {
    fn from(status: tonic::Status) -> Self {
        PeerError::Status {
            code: status.code(),
            message: status.message().to_owned(),
        }
    }
}

/// Errors raised while signing or verifying a message.
#[derive(Debug, Clone, Error)]
pub enum AuthError {
    #[error("missing `{0}`")]
    Missing(&'static str),
    #[error("malformed `{0}`")]
    Malformed(&'static str),
    #[error("message signature is invalid")]
    InvalidSignature,
    #[error("message timestamp is outside the allowed clock skew")]
    StaleTimestamp,
}
