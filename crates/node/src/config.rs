//! Top-level node configuration: a TOML file plus CLI/environment overrides.

use std::path::{Path, PathBuf};

use anyhow::Context;
use net::config::{ClientConfig, P2pConfig};
use serde::{Deserialize, Serialize};

use crate::cli::Cli;

/// Full node configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Stable identifier for this node.
    pub node_id: String,
    pub identity: IdentityConfig,
    pub log: LogConfig,
    pub p2p: P2pConfig,
    pub client: ClientConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node_id: "node-1".to_owned(),
            identity: IdentityConfig::default(),
            log: LogConfig::default(),
            p2p: P2pConfig::default(),
            client: ClientConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from `path`, or return defaults when no path is set.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        match path {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading config file {}", path.display()))?;
                let config = toml::from_str(&text)
                    .with_context(|| format!("parsing config file {}", path.display()))?;
                Ok(config)
            }
            None => Ok(Self::default()),
        }
    }

    /// Apply command-line/environment overrides on top of the file.
    pub fn apply_overrides(&mut self, cli: &Cli) {
        if let Some(node_id) = &cli.node_id {
            self.node_id = node_id.clone();
        }
        if let Some(key_file) = &cli.identity_file {
            self.identity.key_file = key_file.clone();
        }
        if let Some(listen) = cli.p2p_listen {
            self.p2p.listen = listen;
        }
        if let Some(advertise) = &cli.p2p_advertise {
            self.p2p.advertise_address = advertise.clone();
        }
        if let Some(listen) = cli.client_listen {
            self.client.listen = listen;
        }
        if let Some(peers) = &cli.peers {
            self.p2p.peers = peers.clone();
        }
        if let Some(level) = &cli.log {
            self.log.level = level.clone();
        }
    }
}

/// Identity (Ed25519 key) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentityConfig {
    /// File the node's signing key is stored in; created if absent.
    pub key_file: PathBuf,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            key_file: PathBuf::from("node.key"),
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// `tracing` filter directive, e.g. `info,net=debug`.
    pub level: String,
    pub format: LogFormat,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_owned(),
            format: LogFormat::Pretty,
        }
    }
}

/// Output format for logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}
