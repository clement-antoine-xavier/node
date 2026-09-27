//! Cooperative shutdown built on [`tokio_util::sync::CancellationToken`].

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// A cloneable shutdown handle. Every server and background task observes the
/// same token, so a single signal drains the whole node.
#[derive(Debug, Clone, Default)]
pub struct Shutdown {
    token: CancellationToken,
}

impl Shutdown {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// A clone of the underlying cancellation token.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Trigger shutdown.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Resolve once shutdown has been requested.
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }
}

/// Spawn a task that cancels `shutdown` on the first SIGINT/SIGTERM.
pub fn spawn_signal_handler(shutdown: Shutdown) -> JoinHandle<()> {
    tokio::spawn(async move {
        wait_for_signal().await;
        tracing::info!("shutdown signal received, draining");
        shutdown.cancel();
    })
}

#[cfg(unix)]
async fn wait_for_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = sigint.recv() => tracing::info!("received SIGINT"),
        _ = sigterm.recv() => tracing::info!("received SIGTERM"),
    }
}

#[cfg(not(unix))]
async fn wait_for_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
