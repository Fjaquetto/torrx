use std::path::Path;

use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub enum LogGuard {
    File(WorkerGuard),
    Disabled,
}

pub fn init(log_dir: &Path, level: &str) -> LogGuard {
    match init_file_appender(log_dir, level) {
        Ok(guard) => LogGuard::File(guard),
        Err(error) => {
            eprintln!("warning: file logging disabled: {error:#}");
            LogGuard::Disabled
        }
    }
}

fn init_file_appender(log_dir: &Path, level: &str) -> anyhow::Result<WorkerGuard> {
    crate::platform::create_private_dir(log_dir)
        .with_context(|| format!("cannot create log directory {}", log_dir.display()))?;

    let appender = tracing_appender::rolling::daily(log_dir, "torrx.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false),
        )
        .init();

    Ok(guard)
}
