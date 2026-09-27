//! Command-line interface.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "node", version, about = "Decentralized database node")]
pub struct Cli {
    /// Optional subcommand. Without one, the node runs as a server.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Path to a TOML configuration file.
    #[arg(long, env = "NODE_CONFIG", value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Node identifier.
    #[arg(long, env = "NODE_ID", value_name = "ID")]
    pub node_id: Option<String>,
    /// File holding this node's Ed25519 signing key.
    #[arg(long, env = "NODE_IDENTITY_FILE", value_name = "FILE")]
    pub identity_file: Option<PathBuf>,
    /// Address the peer (gRPC) server binds to.
    #[arg(long, env = "NODE_P2P_LISTEN", value_name = "ADDR")]
    pub p2p_listen: Option<SocketAddr>,
    /// Address advertised to peers.
    #[arg(long, env = "NODE_P2P_ADVERTISE", value_name = "ADDR")]
    pub p2p_advertise: Option<String>,
    /// Address the client (HTTP) server binds to.
    #[arg(long, env = "NODE_CLIENT_LISTEN", value_name = "ADDR")]
    pub client_listen: Option<SocketAddr>,
    /// Comma-separated bootstrap peer URIs.
    #[arg(long, env = "NODE_PEERS", value_delimiter = ',', value_name = "URI")]
    pub peers: Option<Vec<String>>,
    /// Log filter, e.g. `info,net=debug`.
    #[arg(long, env = "NODE_LOG", value_name = "FILTER")]
    pub log: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print signed headers for a client-to-node request (for curl/tools).
    Sign(SignArgs),
}

#[derive(Debug, Args)]
pub struct SignArgs {
    /// Signing key file (created if it does not exist).
    #[arg(
        long,
        env = "NODE_IDENTITY_FILE",
        value_name = "FILE",
        default_value = "node.key"
    )]
    pub key_file: PathBuf,
    /// HTTP method.
    #[arg(long, default_value = "GET")]
    pub method: String,
    /// Request path (including query string, if any).
    #[arg(long)]
    pub path: String,
}
