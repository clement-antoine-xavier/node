//! Integration tests for the node-to-client HTTP interface.

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::time::Duration;

use net::client::NodeInfo;
use net::config::ClientConfig;
use net::shutdown::Shutdown;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("local addr")
        .port()
}

fn config(port: u16) -> ClientConfig {
    ClientConfig {
        listen: format!("127.0.0.1:{port}").parse().expect("valid addr"),
        ..ClientConfig::default()
    }
}

async fn start_server(port: u16, shutdown: Shutdown) {
    let info = NodeInfo::new(
        "peer-a",
        "test",
        net::PROTOCOL_VERSION,
        Arc::new(AtomicU32::new(0)),
    );
    let token = shutdown.token();
    let config = config(port);
    tokio::spawn(async move {
        if let Err(error) = net::client::serve(config, info, token).await {
            eprintln!("server error: {error}");
        }
    });
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("client server never became reachable");
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

#[tokio::test]
async fn health_endpoint_reports_ok() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let response = request(port, "GET", "/health").await;
    assert!(response.contains("200 OK"), "response was: {response}");
    assert!(
        response.contains("\"status\":\"ok\""),
        "response was: {response}"
    );

    shutdown.cancel();
}

#[tokio::test]
async fn status_endpoint_reports_node_info() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let response = request(port, "GET", "/status").await;
    assert!(response.contains("200 OK"), "response was: {response}");
    assert!(
        response.contains("\"peer_id\":\"peer-a\""),
        "response was: {response}"
    );
    assert!(
        response.contains("\"protocol_version\":\"1\""),
        "response was: {response}"
    );

    shutdown.cancel();
}

#[tokio::test]
async fn unknown_route_is_not_found() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let response = request(port, "GET", "/nope").await;
    assert!(
        response.contains("404 Not Found"),
        "response was: {response}"
    );

    shutdown.cancel();
}

#[tokio::test]
async fn disallowed_method_is_rejected() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let response = request(port, "POST", "/health").await;
    assert!(
        response.contains("405 Method Not Allowed"),
        "response was: {response}"
    );

    shutdown.cancel();
}
