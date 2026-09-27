//! Node composition: build the servers, optionally dial bootstrap peers, and
//! run until shutdown.

use anyhow::{Context, Result};
use net::client::NodeInfo;
use net::identity::Keypair;
use net::p2p::{NodeServiceImpl, PROTOCOL_VERSION, PeerPool};
use net::shutdown::Shutdown;
use proto::v1;

use crate::config::Config;

/// Node version reported to peers and clients.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct App {
    config: Config,
    shutdown: Shutdown,
}

impl App {
    pub fn new(config: Config, shutdown: Shutdown) -> Self {
        Self { config, shutdown }
    }

    pub async fn run(self) -> Result<()> {
        let keypair =
            Keypair::load_or_generate(&self.config.identity.key_file).with_context(|| {
                format!(
                    "loading identity key from {}",
                    self.config.identity.key_file.display()
                )
            })?;

        let node_service = NodeServiceImpl::new(
            self.config.node_id.clone(),
            self.config.p2p.advertise_address.clone(),
            VERSION,
            keypair.clone(),
            self.config.p2p.max_clock_skew(),
        );
        let peer_count = node_service.peer_count_handle();

        tracing::info!(
            node_id = %self.config.node_id,
            version = VERSION,
            public_key = %keypair.public_key(),
            p2p_listen = %self.config.p2p.listen,
            client_listen = %self.config.client.listen,
            "starting node"
        );

        let token = self.shutdown.token();
        let p2p = tokio::spawn(net::p2p::serve(
            self.config.p2p.clone(),
            node_service,
            token.clone(),
        ));

        let info = NodeInfo::new(
            self.config.node_id.clone(),
            VERSION,
            PROTOCOL_VERSION,
            peer_count,
        );
        let client = tokio::spawn(net::client::serve(self.config.client.clone(), info, token));

        if !self.config.p2p.peers.is_empty() {
            self.dial_bootstrap_peers(keypair).await;
        }

        self.shutdown.cancelled().await;
        tracing::info!("shutting down");

        let _ = tokio::join!(p2p, client);
        Ok(())
    }

    /// Connect to configured peers and exchange a handshake and ping, proving
    /// the outbound `tower` stack end to end.
    async fn dial_bootstrap_peers(&self, keypair: Keypair) {
        let pool = match PeerPool::connect(&self.config.p2p, keypair).await {
            Ok(pool) => pool,
            Err(error) => {
                tracing::warn!(%error, "could not connect to any bootstrap peer");
                return;
            }
        };
        tracing::info!(peers = pool.peer_count(), "connected to bootstrap peers");

        let handshake = v1::HandshakeRequest {
            peer_id: self.config.node_id.clone(),
            protocol_version: PROTOCOL_VERSION.to_owned(),
            advertise_address: self.config.p2p.advertise_address.clone(),
            public_key: String::new(),
        };
        match pool.handshake(handshake).await {
            Ok(response) => tracing::info!(
                remote_peer_id = %response.peer_id,
                remote_public_key = %response.public_key,
                remote_node_version = %response.node_version,
                "handshake complete"
            ),
            Err(error) => tracing::warn!(%error, "handshake failed"),
        }

        match pool.ping(1).await {
            Ok(pong) => tracing::info!(nonce = pong.nonce, "ping complete"),
            Err(error) => tracing::warn!(%error, "ping failed"),
        }
    }
}
