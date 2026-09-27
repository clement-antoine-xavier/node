//! Retry policy for peer calls.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tower::retry::Policy;

use crate::error::PeerError;
use crate::p2p::rpc::{PeerRequest, PeerResponse};

/// Retries retryable failures up to `max_retries`, sleeping for a backoff that
/// grows linearly with the attempt number.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    max_retries: u32,
    attempts: u32,
    backoff: Duration,
}

impl RetryPolicy {
    pub fn new(max_retries: u32, backoff: Duration) -> Self {
        Self {
            max_retries,
            attempts: 0,
            backoff,
        }
    }
}

impl Policy<PeerRequest, PeerResponse, PeerError> for RetryPolicy {
    type Future = Pin<Box<dyn Future<Output = ()> + Send>>;

    fn retry(
        &mut self,
        _request: &mut PeerRequest,
        result: &mut Result<PeerResponse, PeerError>,
    ) -> Option<Self::Future> {
        match result {
            Ok(_) => {
                self.attempts = 0;
                None
            }
            Err(error) if error.is_retryable() && self.attempts < self.max_retries => {
                self.attempts += 1;
                let delay = self.backoff * self.attempts;
                tracing::debug!(
                    attempt = self.attempts,
                    ?delay,
                    error = %error,
                    "retrying peer request"
                );
                Some(Box::pin(tokio::time::sleep(delay)))
            }
            Err(_) => {
                self.attempts = 0;
                None
            }
        }
    }

    fn clone_request(&mut self, request: &PeerRequest) -> Option<PeerRequest> {
        Some(request.clone())
    }
}
