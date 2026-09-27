//! A typed, middleware-friendly façade over the generated gRPC client.
//!
//! Wrapping the generated client in our own request/response enums lets the
//! whole peer stack be composed from `tower` services with a single error type.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use proto::v1;
use tonic::transport::Channel;
use tower::Service;

use crate::error::PeerError;

/// Which peer RPC a request maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerMethod {
    Handshake,
    Ping,
    Status,
}

/// A node-to-node request.
#[derive(Debug, Clone)]
pub enum PeerRequest {
    Handshake(v1::HandshakeRequest),
    Ping(u64),
    Status,
}

impl PeerRequest {
    pub fn handshake(request: v1::HandshakeRequest) -> Self {
        PeerRequest::Handshake(request)
    }

    pub fn ping(nonce: u64) -> Self {
        PeerRequest::Ping(nonce)
    }

    pub fn status() -> Self {
        PeerRequest::Status
    }

    pub fn method(&self) -> PeerMethod {
        match self {
            PeerRequest::Handshake(_) => PeerMethod::Handshake,
            PeerRequest::Ping(_) => PeerMethod::Ping,
            PeerRequest::Status => PeerMethod::Status,
        }
    }
}

/// A node-to-node response.
#[derive(Debug, Clone)]
pub enum PeerResponse {
    Handshake(v1::HandshakeResponse),
    Pong(v1::PongResponse),
    Status(v1::StatusResponse),
}

/// A `tower` service that issues one RPC on a single gRPC channel.
#[derive(Clone)]
pub struct ChannelRpc {
    client: v1::node_service_client::NodeServiceClient<Channel>,
}

impl ChannelRpc {
    pub fn new(channel: Channel) -> Self {
        Self {
            client: v1::node_service_client::NodeServiceClient::new(channel),
        }
    }
}

impl Service<PeerRequest> for ChannelRpc {
    type Response = PeerResponse;
    type Error = PeerError;
    type Future = Pin<Box<dyn Future<Output = Result<PeerResponse, PeerError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: PeerRequest) -> Self::Future {
        let mut client = self.client.clone();
        Box::pin(async move {
            match request {
                PeerRequest::Handshake(request) => {
                    let response = client.handshake(request).await?;
                    Ok(PeerResponse::Handshake(response.into_inner()))
                }
                PeerRequest::Ping(nonce) => {
                    let response = client.ping(v1::PingRequest { nonce }).await?;
                    Ok(PeerResponse::Pong(response.into_inner()))
                }
                PeerRequest::Status => {
                    let response = client.status(v1::StatusRequest {}).await?;
                    Ok(PeerResponse::Status(response.into_inner()))
                }
            }
        })
    }
}
