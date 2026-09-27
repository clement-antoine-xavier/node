//! Integration tests for the node-to-client HTTP interface (signed requests).

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::time::Duration;

use net::auth;
use net::client::NodeInfo;
use net::config::ClientConfig;
use net::identity::Keypair;
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

/// Signature headers for a GET request, signed with a fresh key.
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

#[tokio::test]
async fn health_endpoint_is_open() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let response = request(port, "GET", "/health", &[]).await;
    assert!(response.contains("200 OK"), "response was: {response}");
    assert!(
        response.contains("\"status\":\"ok\""),
        "response was: {response}"
    );

    shutdown.cancel();
}

#[tokio::test]
async fn status_endpoint_accepts_a_signed_request() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let keypair = Keypair::generate().expect("keypair");
    let response = request(port, "GET", "/status", &sign(&keypair, "/status")).await;
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
async fn unsigned_request_is_unauthorized() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let response = request(port, "GET", "/status", &[]).await;
    assert!(
        response.contains("401 Unauthorized"),
        "response was: {response}"
    );

    shutdown.cancel();
}

#[tokio::test]
async fn signature_for_another_path_is_unauthorized() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    // A signature is bound to the request path, so one for `/health` must not
    // authenticate a request for `/status`.
    let keypair = Keypair::generate().expect("keypair");
    let response = request(port, "GET", "/status", &sign(&keypair, "/health")).await;
    assert!(
        response.contains("401 Unauthorized"),
        "response was: {response}"
    );

    shutdown.cancel();
}

#[tokio::test]
async fn unknown_route_is_not_found() {
    let port = free_port();
    let shutdown = Shutdown::new();
    start_server(port, shutdown.clone()).await;

    let keypair = Keypair::generate().expect("keypair");
    let response = request(port, "GET", "/nope", &sign(&keypair, "/nope")).await;
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

    // The method filter runs before signature verification, so no signature is
    // needed to observe the 405.
    let response = request(port, "POST", "/health", &[]).await;
    assert!(
        response.contains("405 Method Not Allowed"),
        "response was: {response}"
    );

    shutdown.cancel();
}
