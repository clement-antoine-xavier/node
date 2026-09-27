//! A minimal, `tower`-friendly HTTP router.
//!
//! It is intentionally hand-rolled: the only job here is to turn a request into
//! a response, leaving middleware to the `tower` stack in [`super::server`].

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use futures::future::{Ready, ready};
use http::{Method, Request, Response};
use tower::Service;

use crate::client::handlers::{self, Body};

/// Transport-level facts about the local node, shared with HTTP handlers.
#[derive(Clone)]
pub struct NodeInfo {
    pub peer_id: Arc<str>,
    pub node_version: Arc<str>,
    pub protocol_version: Arc<str>,
    pub started_at: Instant,
    pub peer_count: Arc<AtomicU32>,
}

impl NodeInfo {
    pub fn new(
        peer_id: impl Into<String>,
        node_version: impl Into<String>,
        protocol_version: impl Into<String>,
        peer_count: Arc<AtomicU32>,
    ) -> Self {
        Self {
            peer_id: Arc::from(peer_id.into()),
            node_version: Arc::from(node_version.into()),
            protocol_version: Arc::from(protocol_version.into()),
            started_at: Instant::now(),
            peer_count,
        }
    }

    pub fn uptime_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub fn peer_count(&self) -> u32 {
        self.peer_count.load(Ordering::Relaxed)
    }
}

/// Routes requests to handlers. Cheap to clone.
#[derive(Clone)]
pub struct Router {
    info: Arc<NodeInfo>,
}

impl Router {
    pub fn new(info: NodeInfo) -> Self {
        Self {
            info: Arc::new(info),
        }
    }
}

impl<B> Service<Request<B>> for Router {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Ready<Result<Response<Body>, Infallible>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let info = self.info.clone();
        let response = match (request.method(), request.uri().path()) {
            (&Method::GET, "/health") => handlers::health(),
            (&Method::GET, "/status") => handlers::status(&info),
            (&Method::GET, "/v1/peers") => handlers::peers(&info),
            _ => handlers::not_found(),
        };
        ready(Ok(response))
    }
}
