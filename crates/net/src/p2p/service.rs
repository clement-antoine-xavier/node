//! Server-side implementation of the generated `NodeService`.
//!
//! Every inbound request is verified against the sender's Ed25519 key before it
//! is served, and every response is signed with this node's key.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use identity::{Keypair, PublicKey, now_ms};
use prost::Message;
use proto::v1;
use proto::v1::node_service_server::NodeService;
use tonic::{Request, Response, Status};

pub use crate::protocol::PROTOCOL_VERSION;

use crate::auth::{self, SignedEnvelope};

const PATH_HANDSHAKE: &str = "/node.v1.NodeService/Handshake";
const PATH_PING: &str = "/node.v1.NodeService/Ping";
const PATH_STATUS: &str = "/node.v1.NodeService/Status";

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
    keypair: Keypair,
    max_skew: Duration,
}

impl NodeServiceImpl {
    pub fn new(
        peer_id: impl Into<String>,
        advertise_address: impl Into<String>,
        node_version: impl Into<String>,
        keypair: Keypair,
        max_skew: Duration,
    ) -> Self {
        Self {
            peer_id: Arc::from(peer_id.into()),
            protocol_version: Arc::from(PROTOCOL_VERSION),
            node_version: Arc::from(node_version.into()),
            advertise_address: Arc::from(advertise_address.into()),
            started_at: Instant::now(),
            peer_count: Arc::new(AtomicU32::new(0)),
            keypair,
            max_skew,
        }
    }

    /// Shared handle used by the client-facing server to report peer counts.
    pub fn peer_count_handle(&self) -> Arc<AtomicU32> {
        self.peer_count.clone()
    }

    /// This node's signing public key.
    pub fn public_key(&self) -> PublicKey {
        self.keypair.public_key()
    }

    /// Address this node advertises to peers.
    pub fn advertise_address(&self) -> &str {
        &self.advertise_address
    }

    fn uptime_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    /// Verify the signature attached to an inbound request, returning the
    /// authenticated sender.
    fn authenticate<M: Message>(
        &self,
        request: &Request<M>,
        path: &str,
    ) -> Result<PublicKey, Status> {
        let envelope = request
            .extensions()
            .get::<SignedEnvelope>()
            .ok_or_else(|| Status::unauthenticated("missing peer credentials"))?;
        auth::verify_message(
            envelope,
            auth::REQUEST_DOMAIN,
            path,
            &request.get_ref().encode_to_vec(),
            self.max_skew,
            now_ms(),
        )
        .map_err(|error| Status::unauthenticated(error.to_string()))?;
        Ok(envelope.public_key)
    }

    /// Wrap an outgoing message in a signed response.
    fn signed_response<M: Message>(&self, path: &str, message: M) -> Response<M> {
        let envelope = auth::sign_message(
            &self.keypair,
            auth::RESPONSE_DOMAIN,
            path,
            &message.encode_to_vec(),
            now_ms(),
        );
        let mut response = Response::new(message);
        if let Err(error) = auth::insert_envelope(response.metadata_mut(), &envelope) {
            tracing::error!(%error, "failed to sign peer response");
        }
        response
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
        let signing_key = self.authenticate(&request, PATH_HANDSHAKE)?.to_hex();
        let remote = request.into_inner();

        // The identity claimed in the message must match the signing key.
        if !remote.public_key.is_empty() && remote.public_key != signing_key {
            return Err(Status::unauthenticated(
                "handshake public key does not match the signing key",
            ));
        }

        tracing::info!(
            remote_peer_id = %remote.peer_id,
            remote_public_key = %signing_key,
            remote_protocol_version = %remote.protocol_version,
            remote_address = %remote.advertise_address,
            "peer handshake received"
        );
        self.peer_count.fetch_add(1, Ordering::Relaxed);

        Ok(self.signed_response(
            PATH_HANDSHAKE,
            v1::HandshakeResponse {
                peer_id: self.peer_id.to_string(),
                protocol_version: self.protocol_version.to_string(),
                node_version: self.node_version.to_string(),
                uptime_ms: self.uptime_ms(),
                peer_count: self.peer_count.load(Ordering::Relaxed),
                public_key: self.keypair.public_key().to_hex(),
            },
        ))
    }

    async fn ping(
        &self,
        request: Request<v1::PingRequest>,
    ) -> Result<Response<v1::PongResponse>, Status> {
        self.authenticate(&request, PATH_PING)?;
        let nonce = request.into_inner().nonce;
        tracing::debug!(nonce, "ping");
        Ok(self.signed_response(
            PATH_PING,
            v1::PongResponse {
                nonce,
                server_time_ms: server_time_ms(),
            },
        ))
    }

    async fn status(
        &self,
        request: Request<v1::StatusRequest>,
    ) -> Result<Response<v1::StatusResponse>, Status> {
        self.authenticate(&request, PATH_STATUS)?;
        Ok(self.signed_response(
            PATH_STATUS,
            v1::StatusResponse {
                peer_id: self.peer_id.to_string(),
                node_version: self.node_version.to_string(),
                uptime_ms: self.uptime_ms(),
                peer_count: self.peer_count.load(Ordering::Relaxed),
                height: 0,
                peers: Vec::new(),
            },
        ))
    }
}
