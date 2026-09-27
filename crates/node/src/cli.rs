//! Command-line interface.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "node", version, about = "Decentralized database node")]
pub struct Cli {
    /// Path to a TOML configuration file.
    #[arg(long, env = "NODE_CONFIG", value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Node identifier.
    #[arg(long, env = "NODE_ID", value_name = "ID")]
    pub node_id: Option<String>,
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
