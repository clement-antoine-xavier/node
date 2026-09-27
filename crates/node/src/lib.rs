//! Decentralized database node.
//!
//! At this stage the node is transport-only: it speaks gRPC to peers and HTTP
//! to clients, signs and verifies every message with an Ed25519 identity, logs,
//! and shuts down cleanly. Consensus and storage land in later milestones.

pub mod app;
pub mod cli;
pub mod config;
pub mod sign;
pub mod telemetry;
