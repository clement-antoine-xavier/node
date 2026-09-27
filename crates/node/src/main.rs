//! The `node` binary: parse configuration, install logging, run the node, or
//! run a helper subcommand.

use anyhow::Result;
use clap::Parser;
use net::shutdown::{Shutdown, spawn_signal_handler};
use node::app::App;
use node::cli::{Cli, Command};
use node::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Command::Sign(args)) = &cli.command {
        return node::sign::run(args);
    }

    let mut config = Config::load(cli.config.as_deref())?;
    config.apply_overrides(&cli);
    node::telemetry::init(&config.log)?;

    let shutdown = Shutdown::new();
    let _signal = spawn_signal_handler(shutdown.clone());

    App::new(config, shutdown).run().await
}
