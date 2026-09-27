//! The node-to-node gRPC server.

use proto::v1::node_service_server::NodeServiceServer;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;

use crate::config::P2pConfig;
use crate::error::NetError;
use crate::p2p::service::NodeServiceImpl;

/// Serve the peer interface until `shutdown` is cancelled.
pub async fn serve(
    config: P2pConfig,
    service: NodeServiceImpl,
    shutdown: CancellationToken,
) -> Result<(), NetError> {
    let addr = config.listen;
    let router = Server::builder()
        .concurrency_limit_per_connection(config.max_concurrent_requests)
        .timeout(config.request_timeout())
        .add_service(
            NodeServiceServer::new(service)
                .max_decoding_message_size(config.max_message_bytes)
                .max_encoding_message_size(config.max_message_bytes),
        );

    tracing::info!(%addr, "p2p gRPC server listening");
    router
        .serve_with_shutdown(addr, async move { shutdown.cancelled().await })
        .await?;
    tracing::info!(%addr, "p2p gRPC server stopped");
    Ok(())
}
