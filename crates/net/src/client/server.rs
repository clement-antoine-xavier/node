//! The node-to-client HTTP server, built directly on `hyper`.

use std::time::Duration;

use http::Method;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower::buffer::Buffer;
use tower::limit::{ConcurrencyLimitLayer, RateLimitLayer};
use tower::timeout::TimeoutLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::client::layers::{MethodFilterLayer, SignatureLayer};
use crate::client::router::{NodeInfo, Router};
use crate::config::ClientConfig;
use crate::error::NetError;

/// Serve the client interface until `shutdown` is cancelled.
pub async fn serve(
    config: ClientConfig,
    info: NodeInfo,
    shutdown: CancellationToken,
) -> Result<(), NetError> {
    let listener = TcpListener::bind(config.listen).await?;
    tracing::info!(addr = %config.listen, "client HTTP server listening");

    // Inner stack: router -> signature check -> method filter -> concurrency
    // limit -> rate limit -> body limit. The buffer at the edge makes the whole
    // thing cheap to clone (and is the only piece that isn't `Clone`).
    let limited = ServiceBuilder::new()
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(ConcurrencyLimitLayer::new(config.max_concurrent_requests))
        .layer(RateLimitLayer::new(
            config.rate_limit_per_second,
            Duration::from_secs(1),
        ))
        .layer(MethodFilterLayer::new([Method::GET]))
        .layer(SignatureLayer::new(
            config.require_signature,
            config.max_clock_skew(),
        ))
        .service(Router::new(info));
    let buffered = Buffer::new(limited, config.max_concurrent_requests);

    // Outer stack: trace -> timeout.
    let service = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_millis(
            config.request_timeout_ms,
        )))
        .service(buffered);

    let hyper_service = TowerToHyperService::new(service);
    let mut connections = JoinSet::new();

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("client HTTP server shutting down");
                break;
            }
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        tracing::warn!(%error, "client accept failed");
                        continue;
                    }
                };
                let io = TokioIo::new(stream);
                let service = hyper_service.clone();
                connections.spawn(async move {
                    let builder = auto::Builder::new(TokioExecutor::new());
                    if let Err(error) = builder.serve_connection(io, service).await {
                        tracing::debug!(%peer, %error, "client connection ended with error");
                    }
                });
            }
        }
    }

    while connections.join_next().await.is_some() {}
    tracing::info!("client HTTP server stopped");
    Ok(())
}
