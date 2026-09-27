//! HTTP handlers. They are synchronous and return fully-buffered bodies.

use bytes::Bytes;
use http::{Response, StatusCode};
use http_body_util::Full;
use serde_json::json;

use crate::client::router::NodeInfo;

/// Response body used across the client interface.
pub type Body = Full<Bytes>;

fn json(status: StatusCode, value: serde_json::Value) -> Response<Body> {
    let bytes = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
    Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(bytes)))
        .expect("valid response")
}

/// `GET /health` — liveness.
pub fn health() -> Response<Body> {
    json(StatusCode::OK, json!({ "status": "ok" }))
}

/// `GET /status` — node and network summary.
pub fn status(info: &NodeInfo) -> Response<Body> {
    json(
        StatusCode::OK,
        json!({
            "peer_id": info.peer_id.as_ref(),
            "node_version": info.node_version.as_ref(),
            "protocol_version": info.protocol_version.as_ref(),
            "uptime_ms": info.uptime_ms(),
            "peer_count": info.peer_count(),
            "height": 0,
        }),
    )
}

/// `GET /v1/peers` — connected peers (placeholder until peer tracking lands).
pub fn peers(info: &NodeInfo) -> Response<Body> {
    json(
        StatusCode::OK,
        json!({
            "peer_count": info.peer_count(),
            "peers": [],
        }),
    )
}

/// Fallback for unknown routes.
pub fn not_found() -> Response<Body> {
    json(StatusCode::NOT_FOUND, json!({ "error": "not found" }))
}

/// Used by the method filter when a known route is called with a bad method.
pub fn method_not_allowed() -> Response<Body> {
    json(
        StatusCode::METHOD_NOT_ALLOWED,
        json!({ "error": "method not allowed" }),
    )
}
