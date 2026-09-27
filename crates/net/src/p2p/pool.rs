//! A pool of peer connections with the full `tower` middleware stack:
//! retry, timeout, filtering, concurrency limiting, rate limiting and
//! power-of-two-choices load balancing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use identity::Keypair;
use proto::v1;
use tonic::transport::Channel;
use tower::balance::p2c::Balance;
use tower::buffer::Buffer;
use tower::discover::ServiceList;
use tower::filter::{FilterLayer, Predicate};
use tower::limit::{ConcurrencyLimitLayer, RateLimitLayer};
use tower::load::Load;
use tower::retry::RetryLayer;
use tower::timeout::TimeoutLayer;
use tower::util::{BoxCloneService, MapErrLayer};
use tower::{BoxError, Service, ServiceBuilder, ServiceExt};

use crate::config::P2pConfig;
use crate::error::{NetError, PeerError};
use crate::p2p::client::connect_channel;
use crate::p2p::policy::RetryPolicy;
use crate::p2p::rpc::{ChannelRpc, PeerMethod, PeerRequest, PeerResponse};

/// Per-peer service after every layer (and type) has been erased.
type ErasedPeer = BoxCloneService<PeerRequest, PeerResponse, BoxError>;

/// A layered peer service paired with its in-flight load, so `tower`'s
/// power-of-two-choices balancer can prefer the idle connection.
#[derive(Clone)]
pub struct Peer {
    inner: ErasedPeer,
    inflight: Arc<AtomicUsize>,
}

impl Peer {
    fn new(inner: ErasedPeer) -> Self {
        Self {
            inner,
            inflight: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Service<PeerRequest> for Peer {
    type Response = PeerResponse;
    type Error = BoxError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<PeerResponse, BoxError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: PeerRequest) -> Self::Future {
        let inflight = self.inflight.clone();
        inflight.fetch_add(1, Ordering::Relaxed);
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let result = inner.call(request).await;
            inflight.fetch_sub(1, Ordering::Relaxed);
            result
        })
    }
}

impl Load for Peer {
    type Metric = usize;

    fn load(&self) -> usize {
        self.inflight.load(Ordering::Relaxed)
    }
}

/// Maps the `BoxError` produced by the timeout layer back into [`PeerError`].
fn map_timeout_error(error: BoxError) -> PeerError {
    match error.downcast::<PeerError>() {
        Ok(error) => *error,
        Err(error) => {
            if error.is::<tower::timeout::error::Elapsed>() {
                PeerError::Timeout
            } else {
                PeerError::Transport(error.to_string())
            }
        }
    }
}

/// A connection pool spread across every configured peer.
///
/// Cheap to clone: the heavy `tower` stack lives behind a buffered handle. The
/// handle is wrapped in a mutex so the pool is `Sync` (a `BoxCloneService` is
/// `Send` but not `Sync`), letting it be borrowed across `await` points.
#[derive(Clone)]
pub struct PeerPool {
    inner: Arc<Mutex<BoxCloneService<PeerRequest, PeerResponse, BoxError>>>,
    peers: usize,
}

impl PeerPool {
    /// Connect to all configured peers and build the middleware stack.
    ///
    /// Peers that fail to connect are skipped with a warning; if none remain
    /// this returns [`NetError::NoPeers`].
    pub async fn connect(config: &P2pConfig, keypair: Keypair) -> Result<Self, NetError> {
        let allowed: Arc<[PeerMethod]> = config
            .allowed_methods
            .iter()
            .filter_map(|method| match method.to_ascii_lowercase().as_str() {
                "handshake" => Some(PeerMethod::Handshake),
                "ping" => Some(PeerMethod::Ping),
                "status" => Some(PeerMethod::Status),
                other => {
                    tracing::warn!(method = other, "ignoring unknown peer method in allowlist");
                    None
                }
            })
            .collect();
        let timeout = config.request_timeout();

        let mut peers = Vec::new();
        for uri in &config.peers {
            let channel = match connect_channel(uri).await {
                Ok(channel) => channel,
                Err(error) => {
                    tracing::warn!(%uri, %error, "skipping unreachable peer");
                    continue;
                }
            };
            peers.push(build_peer(
                channel,
                config,
                timeout,
                allowed.clone(),
                keypair.clone(),
            ));
        }

        if peers.is_empty() {
            return Err(NetError::NoPeers);
        }
        let peer_count = peers.len();

        // Power-of-two-choices balancing across peers, then global request
        // limiting, then a buffer at the edge so the pool is cheap to clone.
        let balance = Balance::new(ServiceList::new::<PeerRequest>(peers.into_iter()));
        let limited = ServiceBuilder::new()
            .layer(RateLimitLayer::new(
                config.rate_limit_per_second,
                Duration::from_secs(1),
            ))
            .layer(ConcurrencyLimitLayer::new(config.max_concurrent_requests))
            .service(balance);
        let buffered = Buffer::new(limited, config.max_concurrent_requests);

        Ok(Self {
            inner: Arc::new(Mutex::new(BoxCloneService::new(buffered))),
            peers: peer_count,
        })
    }

    /// Number of peers that were reachable at startup.
    pub fn peer_count(&self) -> usize {
        self.peers
    }

    async fn call(&self, request: PeerRequest) -> Result<PeerResponse, PeerError> {
        let mut service = self.inner.lock().expect("peer pool mutex poisoned").clone();
        service
            .ready()
            .await
            .map_err(map_timeout_error)?
            .call(request)
            .await
            .map_err(map_timeout_error)
    }

    pub async fn handshake(
        &self,
        request: v1::HandshakeRequest,
    ) -> Result<v1::HandshakeResponse, PeerError> {
        match self.call(PeerRequest::handshake(request)).await? {
            PeerResponse::Handshake(response) => Ok(response),
            _ => Err(PeerError::UnexpectedResponse),
        }
    }

    pub async fn ping(&self, nonce: u64) -> Result<v1::PongResponse, PeerError> {
        match self.call(PeerRequest::ping(nonce)).await? {
            PeerResponse::Pong(response) => Ok(response),
            _ => Err(PeerError::UnexpectedResponse),
        }
    }

    pub async fn status(&self) -> Result<v1::StatusResponse, PeerError> {
        match self.call(PeerRequest::status()).await? {
            PeerResponse::Status(response) => Ok(response),
            _ => Err(PeerError::UnexpectedResponse),
        }
    }
}

/// Build the per-peer stack:
///
/// `filter -> retry -> map_err -> timeout -> channel`
fn build_peer(
    channel: Channel,
    config: &P2pConfig,
    timeout: Duration,
    allowed: Arc<[PeerMethod]>,
    keypair: Keypair,
) -> Peer {
    let predicate = MethodAllowlist { allowed };
    let service = ServiceBuilder::new()
        .layer(FilterLayer::new(predicate))
        .layer(RetryLayer::new(RetryPolicy::new(
            config.max_retries,
            config.retry_backoff(),
        )))
        .layer(MapErrLayer::new(map_timeout_error))
        .layer(TimeoutLayer::new(timeout))
        .service(ChannelRpc::new(channel, keypair, config.max_clock_skew()));
    Peer::new(BoxCloneService::new(service))
}

/// Rejects peer RPCs that are not in the configured allowlist.
#[derive(Clone)]
struct MethodAllowlist {
    allowed: Arc<[PeerMethod]>,
}

impl Predicate<PeerRequest> for MethodAllowlist {
    type Request = PeerRequest;

    fn check(&mut self, request: PeerRequest) -> Result<PeerRequest, BoxError> {
        if self.allowed.contains(&request.method()) {
            Ok(request)
        } else {
            Err(format!("peer method {:?} is not allowed", request.method()).into())
        }
    }
}
