//! torrx is a fast, secure and minimal `BitTorrent` client for the terminal.
//!
//! The crate is split into layers: [`domain`] and [`format`] hold pure logic,
//! [`engine`] isolates the torrent engine behind a trait, [`app`] owns the UI
//! state machine, [`ui`] renders it and [`runtime`] wires everything together.

#![forbid(unsafe_code)]

pub mod app;
pub mod cli;
pub mod config;
pub mod domain;
pub mod engine;
pub mod format;
pub mod logging;
pub mod paths;
pub mod platform;
pub mod runtime;
pub mod ui;

use anyhow::Context;

pub async fn run(cli: cli::Cli) -> anyhow::Result<()> {
    let dirs = paths::AppDirs::discover().context("cannot determine config directories")?;
    let config = config::load(cli.config.as_deref(), &dirs, &cli.overrides())?;
    let _log_guard = logging::init(&dirs.log_dir(), &config.log_level);

    if platform::running_as_root() {
        tracing::warn!("running as root; prefer an unprivileged user for torrent traffic");
        if cli.command.is_some() {
            eprintln!("warning: running as root; prefer an unprivileged user");
        }
    }

    let _ = librqbit::try_increase_nofile_limit();

    match cli.command {
        Some(command) => runtime::run_command(command, &config, &dirs).await,
        None => runtime::run_tui(&config, &dirs).await,
    }
}
