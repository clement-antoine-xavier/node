//! In-process end-to-end smoke test.
//!
//! Boots two full nodes (peer gRPC server + client HTTP server each), has
//! `node-b` handshake `node-a`, and checks the whole surface: signed gRPC
//! round-trips, signed HTTP endpoints, routing, method filtering, and graceful
//! shutdown.

use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::time::Duration;

use net::auth;
use net::config::{ClientConfig, P2pConfig};
use net::identity::Keypair;
use net::p2p::PeerPool;
use net::shutdown::Shutdown;
use node::app::App;
use node::config::{Config, IdentityConfig, LogConfig, LogFormat};
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

fn key_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("node-e2e-{}-{name}.key", std::process::id()))
}

fn node_config(
    node_id: &str,
    p2p_port: u16,
    client_port: u16,
    peers: Vec<String>,
    key_name: &str,
) -> Config {
    Config {
        node_id: node_id.to_owned(),
        identity: IdentityConfig {
            key_file: key_file(key_name),
        },
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

async fn request(port: u16, method: &str, path: &str, headers: &[(String, String)]) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
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

fn sign(keypair: &Keypair, path: &str) -> Vec<(String, String)> {
    let signature = auth::sign_http_request(keypair, "GET", path, auth::now_ms());
    vec![
        (auth::CLIENT_PUBLIC_KEY.to_owned(), signature.public_key),
        (
            auth::CLIENT_TIMESTAMP.to_owned(),
            signature.timestamp_ms.to_string(),
        ),
        (auth::CLIENT_SIGNATURE.to_owned(), signature.signature),
    ]
}

/// Poll the signed `/status` until the reported peer count reaches `expected`.
async fn wait_for_peer_count(port: u16, keypair: &Keypair, expected: u32) -> String {
    let needle = format!("\"peer_count\":{expected}");
    for _ in 0..100 {
        let status = request(port, "GET", "/status", &sign(keypair, "/status")).await;
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
    let client_key = Keypair::generate().expect("client keypair");

    // --- start node-a -----------------------------------------------------
    let shutdown_a = Shutdown::new();
    let node_a = tokio::spawn(
        App::new(
            node_config("node-a", p2p_a, client_a, Vec::new(), "a"),
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
                "b",
            ),
            shutdown_b.clone(),
        )
        .run(),
    );
    wait_for_port(client_b).await;

    // node-b handshakes node-a, so node-a reports one peer.
    let status = wait_for_peer_count(client_a, &client_key, 1).await;
    assert!(
        status.contains("\"peer_id\":\"node-a\""),
        "status: {status}"
    );
    assert!(
        status.contains("\"protocol_version\":\"1\""),
        "status: {status}"
    );

    // --- HTTP surface on node-a -------------------------------------------
    let health = request(client_a, "GET", "/health", &[]).await;
    assert!(
        health.contains("200 OK") && health.contains("\"status\":\"ok\""),
        "health: {health}"
    );

    let peers = request(
        client_a,
        "GET",
        "/v1/peers",
        &sign(&client_key, "/v1/peers"),
    )
    .await;
    assert!(peers.contains("200 OK"), "peers: {peers}");

    let unsigned = request(client_a, "GET", "/status", &[]).await;
    assert!(
        unsigned.contains("401 Unauthorized"),
        "unsigned: {unsigned}"
    );

    let not_found = request(
        client_a,
        "GET",
        "/does-not-exist",
        &sign(&client_key, "/does-not-exist"),
    )
    .await;
    assert!(not_found.contains("404 Not Found"), "404: {not_found}");

    let bad_method = request(client_a, "POST", "/health", &[]).await;
    assert!(
        bad_method.contains("405 Method Not Allowed"),
        "405: {bad_method}"
    );

    // --- signed gRPC directly against node-b ------------------------------
    let pool = PeerPool::connect(
        &P2pConfig {
            peers: vec![format!("http://127.0.0.1:{p2p_b}")],
            ..P2pConfig::default()
        },
        Keypair::generate().expect("peer keypair"),
    )
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

    let _ = std::fs::remove_file(key_file("a"));
    let _ = std::fs::remove_file(key_file("b"));
}
