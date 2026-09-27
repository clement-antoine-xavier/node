//! In-process end-to-end smoke test.
//!
//! Boots two full nodes (peer gRPC server + client HTTP server each), has
//! `node-b` handshake `node-a`, and checks the whole surface: gRPC round-trips,
//! HTTP endpoints, routing, method filtering, and graceful shutdown.

use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

use net::config::{ClientConfig, P2pConfig};
use net::p2p::PeerPool;
use net::shutdown::Shutdown;
use node::app::App;
use node::config::{Config, LogConfig, LogFormat};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().expect("valid addr")
}

fn node_config(node_id: &str, p2p_port: u16, client_port: u16, peers: Vec<String>) -> Config {
    Config {
        node_id: node_id.to_owned(),
        log: LogConfig {
            level: "info".to_owned(),
            format: LogFormat::Pretty,
        },
        p2p: P2pConfig {
            listen: addr(p2p_port),
            peers,
            ..P2pConfig::default()
        },
        client: ClientConfig {
            listen: addr(client_port),
            ..ClientConfig::default()
        },
    }
}

/// Wait until something is listening on `port`.
async fn wait_for_port(port: u16) {
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("nothing listening on port {port}");
}

async fn request(port: u16, method: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    let request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");
    let mut buffer = Vec::new();
    stream
        .read_to_end(&mut buffer)
        .await
        .expect("read response");
    String::from_utf8_lossy(&buffer).into_owned()
}

/// Poll `/status` until the reported peer count reaches `expected`.
async fn wait_for_peer_count(port: u16, expected: u32) -> String {
    let needle = format!("\"peer_count\":{expected}");
    for _ in 0..100 {
        let status = request(port, "GET", "/status").await;
        if status.contains(&needle) {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("peer count never reached {expected}");
}

#[tokio::test]
async fn two_nodes_handshake_and_serve_clients() {
    let p2p_a = free_port();
    let client_a = free_port();
    let p2p_b = free_port();
    let client_b = free_port();

    // --- start node-a -----------------------------------------------------
    let shutdown_a = Shutdown::new();
    let node_a = tokio::spawn(
        App::new(
            node_config("node-a", p2p_a, client_a, Vec::new()),
            shutdown_a.clone(),
        )
        .run(),
    );
    wait_for_port(p2p_a).await;
    wait_for_port(client_a).await;

    // --- start node-b, which dials node-a ---------------------------------
    let shutdown_b = Shutdown::new();
    let node_b = tokio::spawn(
        App::new(
            node_config(
                "node-b",
                p2p_b,
                client_b,
                vec![format!("http://127.0.0.1:{p2p_a}")],
            ),
            shutdown_b.clone(),
        )
        .run(),
    );
    wait_for_port(client_b).await;

    // node-b handshakes node-a, so node-a reports one peer.
    let status = wait_for_peer_count(client_a, 1).await;
    assert!(
        status.contains("\"peer_id\":\"node-a\""),
        "status: {status}"
    );
    assert!(
        status.contains("\"protocol_version\":\"1\""),
        "status: {status}"
    );

    // --- HTTP surface on node-a -------------------------------------------
    let health = request(client_a, "GET", "/health").await;
    assert!(
        health.contains("200 OK") && health.contains("\"status\":\"ok\""),
        "health: {health}"
    );

    let peers = request(client_a, "GET", "/v1/peers").await;
    assert!(peers.contains("200 OK"), "peers: {peers}");

    let not_found = request(client_a, "GET", "/does-not-exist").await;
    assert!(not_found.contains("404 Not Found"), "404: {not_found}");

    let bad_method = request(client_a, "POST", "/health").await;
    assert!(
        bad_method.contains("405 Method Not Allowed"),
        "405: {bad_method}"
    );

    // --- gRPC surface directly against node-b -----------------------------
    let pool = PeerPool::connect(&P2pConfig {
        peers: vec![format!("http://127.0.0.1:{p2p_b}")],
        ..P2pConfig::default()
    })
    .await
    .expect("connect to node-b");
    let pong = pool.ping(99).await.expect("ping node-b");
    assert_eq!(pong.nonce, 99);
    assert_eq!(
        pool.status().await.expect("status node-b").peer_id,
        "node-b"
    );

    // --- graceful shutdown ------------------------------------------------
    shutdown_a.cancel();
    shutdown_b.cancel();
    node_a
        .await
        .expect("node-a task")
        .expect("node-a exits cleanly");
    node_b
        .await
        .expect("node-b task")
        .expect("node-b exits cleanly");
}
