//! Server-side implementation of the generated `NodeService`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use proto::v1;
use proto::v1::node_service_server::NodeService;
use tonic::{Request, Response, Status};

pub use crate::protocol::PROTOCOL_VERSION;

/// A transport-only view of the local node.
///
/// It exposes identity/liveness information and holds no database state yet.
#[derive(Clone)]
pub struct NodeServiceImpl {
    peer_id: Arc<str>,
    protocol_version: Arc<str>,
    node_version: Arc<str>,
    advertise_address: Arc<str>,
    started_at: Instant,
    peer_count: Arc<AtomicU32>,
}

impl NodeServiceImpl {
    pub fn new(
        peer_id: impl Into<String>,
        advertise_address: impl Into<String>,
        node_version: impl Into<String>,
    ) -> Self {
        Self {
            peer_id: Arc::from(peer_id.into()),
            protocol_version: Arc::from(PROTOCOL_VERSION),
            node_version: Arc::from(node_version.into()),
            advertise_address: Arc::from(advertise_address.into()),
            started_at: Instant::now(),
            peer_count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Shared handle used by the client-facing server to report peer counts.
    pub fn peer_count_handle(&self) -> Arc<AtomicU32> {
        self.peer_count.clone()
    }

    /// Address this node advertises to peers.
    pub fn advertise_address(&self) -> &str {
        &self.advertise_address
    }

    fn uptime_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

fn server_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

#[tonic::async_trait]
impl NodeService for NodeServiceImpl {
    async fn handshake(
        &self,
        request: Request<v1::HandshakeRequest>,
    ) -> Result<Response<v1::HandshakeResponse>, Status> {
        let remote = request.into_inner();
        tracing::info!(
            remote_peer_id = %remote.peer_id,
            remote_protocol_version = %remote.protocol_version,
            remote_address = %remote.advertise_address,
            "peer handshake received"
        );
        self.peer_count.fetch_add(1, Ordering::Relaxed);

        Ok(Response::new(v1::HandshakeResponse {
            peer_id: self.peer_id.to_string(),
            protocol_version: self.protocol_version.to_string(),
            node_version: self.node_version.to_string(),
            uptime_ms: self.uptime_ms(),
            peer_count: self.peer_count.load(Ordering::Relaxed),
        }))
    }

    async fn ping(
        &self,
        request: Request<v1::PingRequest>,
    ) -> Result<Response<v1::PongResponse>, Status> {
        let nonce = request.into_inner().nonce;
        tracing::debug!(nonce, "ping");
        Ok(Response::new(v1::PongResponse {
            nonce,
            server_time_ms: server_time_ms(),
        }))
    }

    async fn status(
        &self,
        _request: Request<v1::StatusRequest>,
    ) -> Result<Response<v1::StatusResponse>, Status> {
        Ok(Response::new(v1::StatusResponse {
            peer_id: self.peer_id.to_string(),
            node_version: self.node_version.to_string(),
            uptime_ms: self.uptime_ms(),
            peer_count: self.peer_count.load(Ordering::Relaxed),
            height: 0,
            peers: Vec::new(),
        }))
    }
}
