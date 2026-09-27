//! Networking primitives for the node.
//!
//! This crate knows nothing about consensus or storage. It only knows how to
//! talk to peers (gRPC over [`tonic`], node-to-node) and to clients (HTTP over
//! [`hyper`], node-to-client), and how to apply the shared [`tower`] middleware
//! stack around both directions.

pub mod client;
pub mod config;
pub mod error;
pub mod p2p;
pub mod protocol;
pub mod shutdown;

pub use protocol::{PROTOCOL_VERSION, v1};
