//! A typed, middleware-friendly façade over the generated gRPC client.
//!
//! Wrapping the generated client in our own request/response enums lets the
//! whole peer stack be composed from `tower` services with a single error type.
//! Every request is signed and every response verified, so a peer knows who
//! sent what.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use identity::{Keypair, now_ms};
use prost::Message;
use proto::v1;
use tonic::transport::Channel;
use tower::Service;

use crate::auth;
use crate::error::PeerError;

const PATH_HANDSHAKE: &str = "/node.v1.NodeService/Handshake";
const PATH_PING: &str = "/node.v1.NodeService/Ping";
const PATH_STATUS: &str = "/node.v1.NodeService/Status";

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

/// A `tower` service that signs and issues one RPC on a single gRPC channel.
#[derive(Clone)]
pub struct ChannelRpc {
    client: v1::node_service_client::NodeServiceClient<Channel>,
    keypair: Keypair,
    max_skew: Duration,
}

impl ChannelRpc {
    pub fn new(channel: Channel, keypair: Keypair, max_skew: Duration) -> Self {
        Self {
            client: v1::node_service_client::NodeServiceClient::new(channel),
            keypair,
            max_skew,
        }
    }
}

/// Sign and wrap an outgoing message into a gRPC request.
fn signed_request<M: Message>(
    keypair: &Keypair,
    path: &str,
    message: M,
) -> Result<tonic::Request<M>, PeerError> {
    let envelope = auth::sign_message(
        keypair,
        auth::REQUEST_DOMAIN,
        path,
        &message.encode_to_vec(),
        now_ms(),
    );
    let mut request = tonic::Request::new(message);
    auth::insert_envelope(request.metadata_mut(), &envelope)
        .map_err(|error| PeerError::Signing(error.to_string()))?;
    Ok(request)
}

/// Verify a signed inbound response.
fn verify_response<M: Message>(
    response: &tonic::Response<M>,
    path: &str,
    max_skew: Duration,
) -> Result<(), PeerError> {
    let envelope = auth::envelope_from_metadata(response.metadata())
        .map_err(|error| PeerError::Unauthenticated(error.to_string()))?;
    auth::verify_message(
        &envelope,
        auth::RESPONSE_DOMAIN,
        path,
        &response.get_ref().encode_to_vec(),
        max_skew,
        now_ms(),
    )
    .map_err(|error| PeerError::Unauthenticated(error.to_string()))
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
        let keypair = self.keypair.clone();
        let max_skew = self.max_skew;
        Box::pin(async move {
            match request {
                PeerRequest::Handshake(mut request) => {
                    // Bind the claimed identity to the key that signs the
                    // request; the server checks these match.
                    request.public_key = keypair.public_key().to_hex();
                    let response = client
                        .handshake(signed_request(&keypair, PATH_HANDSHAKE, request)?)
                        .await?;
                    verify_response(&response, PATH_HANDSHAKE, max_skew)?;
                    Ok(PeerResponse::Handshake(response.into_inner()))
                }
                PeerRequest::Ping(nonce) => {
                    let request = v1::PingRequest { nonce };
                    let response = client
                        .ping(signed_request(&keypair, PATH_PING, request)?)
                        .await?;
                    verify_response(&response, PATH_PING, max_skew)?;
                    Ok(PeerResponse::Pong(response.into_inner()))
                }
                PeerRequest::Status => {
                    let request = v1::StatusRequest {};
                    let response = client
                        .status(signed_request(&keypair, PATH_STATUS, request)?)
                        .await?;
                    verify_response(&response, PATH_STATUS, max_skew)?;
                    Ok(PeerResponse::Status(response.into_inner()))
                }
            }
        })
    }
}
