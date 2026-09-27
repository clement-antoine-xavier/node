//! Message signing and verification.
//!
//! The node signs every message it sends and verifies every message it
//! receives. Signatures cover the message contents plus a timestamp (to bound
//! replay) and a context string (to stop a signature for one RPC being replayed
//! as another). See [`identity::canonical_payload`] for the encoding.

use std::time::Duration;

use identity::{Keypair, PublicKey, Signature, canonical_payload, timestamp_field};
use tonic::metadata::{MetadataMap, MetadataValue};

pub use identity::now_ms;

use crate::error::AuthError;

/// gRPC metadata / HTTP header names for peer (node-to-node) signatures.
pub const PEER_PUBLIC_KEY: &str = "x-peer-public-key";
pub const PEER_TIMESTAMP: &str = "x-peer-timestamp";
pub const PEER_SIGNATURE: &str = "x-peer-signature";

/// gRPC metadata / HTTP header names for client (client-to-node) signatures.
pub const CLIENT_PUBLIC_KEY: &str = "x-client-public-key";
pub const CLIENT_TIMESTAMP: &str = "x-client-timestamp";
pub const CLIENT_SIGNATURE: &str = "x-client-signature";

/// Domain separation string for a node-to-node request.
pub const REQUEST_DOMAIN: &str = "node.v1.peer.request";
/// Domain separation string for a node-to-node response.
pub const RESPONSE_DOMAIN: &str = "node.v1.peer.response";
/// Domain separation string for a client-to-node request.
pub const CLIENT_DOMAIN: &str = "node.v1.client.request";

/// A signed envelope: what was sent, by whom, and the proof.
#[derive(Clone, Debug)]
pub struct SignedEnvelope {
    pub public_key: PublicKey,
    pub timestamp_ms: u64,
    pub signature: Signature,
}

/// Request extension inserted once a client's HTTP signature is verified.
#[derive(Clone, Debug)]
pub struct ClientIdentity(pub PublicKey);

/// Sign a node-to-node message for `domain` (request or response) and `kind`
/// (the RPC path).
pub fn sign_message(
    keypair: &Keypair,
    domain: &str,
    kind: &str,
    message: &[u8],
    timestamp_ms: u64,
) -> SignedEnvelope {
    let timestamp = timestamp_field(timestamp_ms);
    let payload = canonical_payload(domain, &[kind.as_bytes(), &timestamp, message]);
    SignedEnvelope {
        public_key: keypair.public_key(),
        timestamp_ms,
        signature: keypair.sign(&payload),
    }
}

/// Verify a node-to-node message, enforcing the clock-skew window.
pub fn verify_message(
    envelope: &SignedEnvelope,
    domain: &str,
    kind: &str,
    message: &[u8],
    max_skew: Duration,
    now: u64,
) -> Result<(), AuthError> {
    check_freshness(envelope.timestamp_ms, max_skew, now)?;
    let timestamp = timestamp_field(envelope.timestamp_ms);
    let payload = canonical_payload(domain, &[kind.as_bytes(), &timestamp, message]);
    envelope
        .public_key
        .verify(&payload, &envelope.signature)
        .map_err(|_| AuthError::InvalidSignature)
}

/// The signed part of a client-to-node HTTP request.
fn http_payload(method: &str, path_and_query: &str, timestamp_ms: u64) -> Vec<u8> {
    let timestamp = timestamp_field(timestamp_ms);
    canonical_payload(
        CLIENT_DOMAIN,
        &[method.as_bytes(), path_and_query.as_bytes(), &timestamp],
    )
}

/// Signature headers a client attaches to an HTTP request.
#[derive(Clone, Debug)]
pub struct HttpSignature {
    pub public_key: String,
    pub timestamp_ms: u64,
    pub signature: String,
}

/// Sign a client-to-node HTTP request.
pub fn sign_http_request(
    keypair: &Keypair,
    method: &str,
    path_and_query: &str,
    timestamp_ms: u64,
) -> HttpSignature {
    let signature = keypair.sign(&http_payload(method, path_and_query, timestamp_ms));
    HttpSignature {
        public_key: keypair.public_key().to_hex(),
        timestamp_ms,
        signature: signature.to_hex(),
    }
}

/// Verify a client-to-node HTTP request, returning the authenticated client.
pub fn verify_http_request(
    method: &str,
    path_and_query: &str,
    timestamp_ms: u64,
    public_key: &str,
    signature: &str,
    max_skew: Duration,
    now: u64,
) -> Result<PublicKey, AuthError> {
    check_freshness(timestamp_ms, max_skew, now)?;
    let public_key =
        PublicKey::from_hex(public_key).map_err(|_| AuthError::Malformed(CLIENT_PUBLIC_KEY))?;
    let signature =
        Signature::from_hex(signature).map_err(|_| AuthError::Malformed(CLIENT_SIGNATURE))?;
    public_key
        .verify(
            &http_payload(method, path_and_query, timestamp_ms),
            &signature,
        )
        .map_err(|_| AuthError::InvalidSignature)?;
    Ok(public_key)
}

fn check_freshness(timestamp_ms: u64, max_skew: Duration, now: u64) -> Result<(), AuthError> {
    if now.abs_diff(timestamp_ms) > max_skew.as_millis() as u64 {
        Err(AuthError::StaleTimestamp)
    } else {
        Ok(())
    }
}

/// Attach a peer signature to gRPC metadata.
pub fn insert_envelope(
    metadata: &mut MetadataMap,
    envelope: &SignedEnvelope,
) -> Result<(), AuthError> {
    let public_key = MetadataValue::try_from(envelope.public_key.to_hex())
        .map_err(|_| AuthError::Malformed(PEER_PUBLIC_KEY))?;
    let timestamp = MetadataValue::try_from(envelope.timestamp_ms.to_string())
        .map_err(|_| AuthError::Malformed(PEER_TIMESTAMP))?;
    let signature = MetadataValue::try_from(envelope.signature.to_hex())
        .map_err(|_| AuthError::Malformed(PEER_SIGNATURE))?;
    metadata.insert(PEER_PUBLIC_KEY, public_key);
    metadata.insert(PEER_TIMESTAMP, timestamp);
    metadata.insert(PEER_SIGNATURE, signature);
    Ok(())
}

/// Extract a peer signature from gRPC metadata.
pub fn envelope_from_metadata(metadata: &MetadataMap) -> Result<SignedEnvelope, AuthError> {
    let public_key =
        metadata_str(metadata, PEER_PUBLIC_KEY).ok_or(AuthError::Missing(PEER_PUBLIC_KEY))?;
    let timestamp =
        metadata_str(metadata, PEER_TIMESTAMP).ok_or(AuthError::Missing(PEER_TIMESTAMP))?;
    let signature =
        metadata_str(metadata, PEER_SIGNATURE).ok_or(AuthError::Missing(PEER_SIGNATURE))?;

    let public_key =
        PublicKey::from_hex(public_key).map_err(|_| AuthError::Malformed(PEER_PUBLIC_KEY))?;
    let timestamp_ms = timestamp
        .parse()
        .map_err(|_| AuthError::Malformed(PEER_TIMESTAMP))?;
    let signature =
        Signature::from_hex(signature).map_err(|_| AuthError::Malformed(PEER_SIGNATURE))?;

    Ok(SignedEnvelope {
        public_key,
        timestamp_ms,
        signature,
    })
}

fn metadata_str<'a>(metadata: &'a MetadataMap, key: &str) -> Option<&'a str> {
    metadata.get(key).and_then(|value| value.to_str().ok())
}

/// Server-side gRPC interceptor: requires a well-formed, fresh signature and
/// makes the sender's identity available to the handler via request extensions.
#[derive(Clone)]
pub struct PeerAuthInterceptor {
    max_skew: Duration,
}

impl PeerAuthInterceptor {
    pub fn new(max_skew: Duration) -> Self {
        Self { max_skew }
    }
}

impl tonic::service::Interceptor for PeerAuthInterceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        let envelope = envelope_from_metadata(request.metadata())
            .map_err(|error| tonic::Status::unauthenticated(error.to_string()))?;
        check_freshness(envelope.timestamp_ms, self.max_skew, now_ms())
            .map_err(|error| tonic::Status::unauthenticated(error.to_string()))?;
        request.extensions_mut().insert(envelope);
        Ok(request)
    }
}
