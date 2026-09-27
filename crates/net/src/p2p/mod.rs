//! Node-to-node networking over gRPC.

pub mod client;
pub mod policy;
pub mod pool;
pub mod rpc;
pub mod server;
pub mod service;

pub use client::connect_channel;
pub use policy::RetryPolicy;
pub use pool::PeerPool;
pub use rpc::{PeerMethod, PeerRequest, PeerResponse};
pub use server::serve;
pub use service::{NodeServiceImpl, PROTOCOL_VERSION};
