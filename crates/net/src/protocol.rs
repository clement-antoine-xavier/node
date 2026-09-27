//! Re-export of the generated protocol so callers can depend on `net` alone.

pub use proto::v1;

/// Version of the peer protocol spoken on the node-to-node interface.
pub const PROTOCOL_VERSION: &str = "1";
