//! Network configuration.
//!
//! The node binary embeds these sections in its own [`Config`], so the
//! networking crate can be configured independently in tests.

use std::net::SocketAddr;
use std::time::Duration;

use serde::{Deserialize, Serialize};

fn default_p2p_listen() -> SocketAddr {
    "127.0.0.1:9001".parse().expect("valid default p2p address")
}

fn default_client_listen() -> SocketAddr {
    "127.0.0.1:8080"
        .parse()
        .expect("valid default client address")
}

fn default_allowed_methods() -> Vec<String> {
    vec!["handshake".into(), "ping".into(), "status".into()]
}

/// Node-to-node (gRPC) settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct P2pConfig {
    /// Address the gRPC server binds to.
    pub listen: SocketAddr,
    /// Address advertised to peers in the handshake.
    pub advertise_address: String,
    /// Static bootstrap peers, e.g. `http://127.0.0.1:9002`.
    pub peers: Vec<String>,
    /// Per-request timeout, in milliseconds.
    pub request_timeout_ms: u64,
    /// Maximum retry attempts for a retryable peer error.
    pub max_retries: u32,
    /// Base retry backoff; the policy multiplies it by the attempt number.
    pub retry_backoff_ms: u64,
    /// Per-connection and per-peer concurrency limit.
    pub max_concurrent_requests: usize,
    /// Per-peer request rate, requests per second.
    pub rate_limit_per_second: u64,
    /// Maximum encoded/decoded gRPC message size, in bytes.
    pub max_message_bytes: usize,
    /// Peer RPCs a connection is allowed to issue; anything else is filtered out.
    pub allowed_methods: Vec<String>,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            listen: default_p2p_listen(),
            advertise_address: String::new(),
            peers: Vec::new(),
            request_timeout_ms: 5_000,
            max_retries: 3,
            retry_backoff_ms: 200,
            max_concurrent_requests: 32,
            rate_limit_per_second: 1_000,
            max_message_bytes: 4 * 1024 * 1024,
            allowed_methods: default_allowed_methods(),
        }
    }
}

impl P2pConfig {
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms)
    }

    pub fn retry_backoff(&self) -> Duration {
        Duration::from_millis(self.retry_backoff_ms)
    }
}

/// Node-to-client (HTTP) settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    /// Address the HTTP server binds to.
    pub listen: SocketAddr,
    /// Per-request timeout, in milliseconds.
    pub request_timeout_ms: u64,
    /// Maximum number of in-flight requests.
    pub max_concurrent_requests: usize,
    /// Request rate, requests per second.
    pub rate_limit_per_second: u64,
    /// Maximum accepted request body size, in bytes.
    pub max_body_bytes: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            listen: default_client_listen(),
            request_timeout_ms: 10_000,
            max_concurrent_requests: 128,
            rate_limit_per_second: 2_000,
            max_body_bytes: 1024 * 1024,
        }
    }
}
