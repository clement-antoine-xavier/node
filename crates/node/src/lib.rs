//! Decentralized database node.
//!
//! At this stage the node is transport-only: it speaks gRPC to peers and HTTP
//! to clients, with logging and graceful shutdown wired up. Consensus and
//! storage land in later milestones.

pub mod app;
pub mod cli;
pub mod config;
pub mod telemetry;
