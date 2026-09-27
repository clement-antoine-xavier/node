//! Protobuf/gRPC definitions shared by every crate.
//!
//! The generated code lives in [`v1`]. The encoded `FileDescriptorSet` is
//! embedded as [`v1::FILE_DESCRIPTOR_SET`] for future reflection support.

/// `node.v1` package: peers, status and liveness.
pub mod v1 {
    tonic::include_proto!("node.v1");

    /// Encoded `FileDescriptorSet` for the `node.v1` package.
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("node_descriptor");
}
