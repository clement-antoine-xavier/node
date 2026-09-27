//! Logging setup.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::{LogConfig, LogFormat};

/// Install the global `tracing` subscriber.
///
/// `RUST_LOG` wins if set; otherwise the configured level is used.
pub fn init(config: &LogConfig) -> anyhow::Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));
    let registry = tracing_subscriber::registry().with(filter);

    match config.format {
        LogFormat::Json => registry
            .with(fmt::layer().json().with_target(true))
            .try_init()?,
        LogFormat::Pretty => registry.with(fmt::layer().with_target(true)).try_init()?,
    }

    Ok(())
}
