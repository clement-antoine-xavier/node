//! Integration tests for the node-to-node gRPC interface (authenticated).

use std::net::TcpListener;
use std::time::Duration;

use net::config::P2pConfig;
use net::error::PeerError;
use net::identity::Keypair;
use net::p2p::{NodeServiceImpl, PeerPool};
use net::shutdown::Shutdown;
use net::v1;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn server_config(port: u16) -> P2pConfig {
    P2pConfig {
        listen: format!("127.0.0.1:{port}").parse().expect("valid addr"),
        ..P2pConfig::default()
    }
}

fn client_config(port: u16) -> P2pConfig {
    P2pConfig {
        peers: vec![format!("http://127.0.0.1:{port}")],
        ..P2pConfig::default()
    }
}

async fn start_server(config: P2pConfig, shutdown: Shutdown) -> tokio::task::JoinHandle<()> {
    let token = shutdown.token();
    tokio::spawn(async move {
        let keypair = Keypair::generate().expect("server keypair");
        let service = NodeServiceImpl::new(
            "peer-a",
            "127.0.0.1:0",
            "test",
            keypair,
            Duration::from_secs(300),
        );
        if let Err(error) = net::p2p::serve(config, service, token).await {
            eprintln!("server error: {error}");
        }
    })
}

async fn connect_with_retry(config: &P2pConfig) -> PeerPool {
    for _ in 0..100 {
        let keypair = Keypair::generate().expect("client keypair");
        if let Ok(pool) = PeerPool::connect(config, keypair).await {
            return pool;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("peer never became reachable");
}

#[tokio::test]
async fn ping_and_status_round_trip() {
    let port = free_port();
    let shutdown = Shutdown::new();
    let server = start_server(server_config(port), shutdown.clone()).await;

    let pool = connect_with_retry(&client_config(port)).await;
    assert_eq!(pool.peer_count(), 1);

    let pong = pool.ping(7).await.expect("ping succeeds");
    assert_eq!(pong.nonce, 7);
    assert!(pong.server_time_ms > 0);

    let status = pool.status().await.expect("status succeeds");
    assert_eq!(status.peer_id, "peer-a");
    assert_eq!(status.node_version, "test");
    assert_eq!(status.height, 0);

    shutdown.cancel();
    server.await.expect("server task");
}

#[tokio::test]
async fn handshake_reveals_the_peer_public_key() {
    let port = free_port();
    let shutdown = Shutdown::new();
    let server = start_server(server_config(port), shutdown.clone()).await;

    let pool = connect_with_retry(&client_config(port)).await;
    let response = pool
        .handshake(v1::HandshakeRequest {
            peer_id: "peer-b".into(),
            protocol_version: net::PROTOCOL_VERSION.into(),
            advertise_address: String::new(),
            public_key: String::new(),
        })
        .await
        .expect("handshake succeeds");

    assert_eq!(response.peer_id, "peer-a");
    assert!(!response.public_key.is_empty(), "server key is exposed");

    shutdown.cancel();
    server.await.expect("server task");
}

#[tokio::test]
async fn unsigned_peer_request_is_rejected() {
    let port = free_port();
    let shutdown = Shutdown::new();
    let server = start_server(server_config(port), shutdown.clone()).await;

    // Wait for the server, then talk to it with a raw, unsigned client.
    let channel = loop {
        if let Ok(channel) = net::p2p::connect_channel(&format!("http://127.0.0.1:{port}")).await {
            break channel;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let mut client = v1::node_service_client::NodeServiceClient::new(channel);
    let status = client
        .ping(v1::PingRequest { nonce: 1 })
        .await
        .expect_err("unsigned ping must be rejected");
    assert_eq!(status.code(), tonic::Code::Unauthenticated);

    shutdown.cancel();
    server.await.expect("server task");
}

#[tokio::test]
async fn disallowed_method_is_filtered_client_side() {
    let port = free_port();
    let shutdown = Shutdown::new();
    let server = start_server(server_config(port), shutdown.clone()).await;

    let mut config = client_config(port);
    config.allowed_methods = vec!["ping".into()];
    let pool = connect_with_retry(&config).await;

    let error = pool.status().await.expect_err("status is filtered out");
    assert!(matches!(error, PeerError::Transport(_)), "got {error:?}");

    pool.ping(1).await.expect("ping is still allowed");

    shutdown.cancel();
    server.await.expect("server task");
}
