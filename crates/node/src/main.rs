//! The `node` binary: parse configuration, install logging, run the node.

use anyhow::Result;
use clap::Parser;
use net::shutdown::{Shutdown, spawn_signal_handler};
use node::app::App;
use node::cli::Cli;
use node::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut config = Config::load(cli.config.as_deref())?;
    config.apply_overrides(&cli);
    node::telemetry::init(&config.log)?;

    let shutdown = Shutdown::new();
    let _signal = spawn_signal_handler(shutdown.clone());

    App::new(config, shutdown).run().await
}
