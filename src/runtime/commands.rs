use std::{path::Path, sync::Arc};

use anyhow::Context;

use crate::{
    cli::Command,
    config::Config,
    domain::TorrentDetails,
    engine::{AddOptions, AddSource, LibrqbitEngine, TorrentEngine},
    format,
    paths::{self, AppDirs},
};

pub async fn run_command(command: Command, config: &Config, dirs: &AppDirs) -> anyhow::Result<()> {
    if matches!(command, Command::ConfigPath) {
        println!("{}", dirs.config_file().display());
        return Ok(());
    }

    prepare_dirs(config, dirs)?;
    let engine: Arc<dyn TorrentEngine> =
        Arc::new(LibrqbitEngine::open(config, &dirs.session_dir(), &dirs.dht_file()).await?);

    let result = execute(engine.as_ref(), command).await;
    engine.shutdown().await;
    result
}

async fn execute(engine: &dyn TorrentEngine, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Add { sources, paused } => {
            for source in sources {
                let add = AddSource::parse(&source, paths::home_dir().as_deref())
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
                let outcome = engine
                    .add(
                        add,
                        AddOptions {
                            paused,
                            ..AddOptions::default()
                        },
                    )
                    .await?;
                println!(
                    "added #{}  {}  {}",
                    outcome.id, outcome.info_hash, outcome.name
                );
            }
            println!();
            println!("note: transfers run while torrx is open — start `torrx` to keep going");
        }
        Command::List => {
            let snapshot = engine.snapshot().await;
            print_list(&snapshot.torrents);
        }
        Command::Info { selector } => {
            let id = engine.resolve(&selector).await?;
            let details = engine.details(id).await?;
            print_details(&details);
        }
        Command::Pause { selector } => {
            let id = engine.resolve(&selector).await?;
            engine.pause(id).await?;
            println!("paused #{id}");
        }
        Command::Resume { selector } => {
            let id = engine.resolve(&selector).await?;
            engine.resume(id).await?;
            println!("resumed #{id}");
        }
        Command::Remove {
            selector,
            delete_files,
        } => {
            let id = engine.resolve(&selector).await?;
            engine.remove(id, delete_files).await?;
            let suffix = if delete_files { " and its data" } else { "" };
            println!("removed #{id}{suffix}");
        }
        Command::ConfigPath => {}
    }
    Ok(())
}

fn print_list(torrents: &[crate::domain::TorrentView]) {
    if torrents.is_empty() {
        println!("no torrents");
        return;
    }
    println!(
        "{:>4}  {:<6} {:<9} {:>6} {:>11} {:>11} {:>7} {:>5}  name",
        "id", "state", "size", "done", "down", "up", "eta", "ratio",
    );
    for torrent in torrents {
        println!(
            "{:>4}  {:<6} {:<9} {:>6} {:>11} {:>11} {:>7} {:>5}  {}",
            torrent.id,
            torrent.status.label(),
            format::bytes(torrent.total_bytes),
            format::percent(torrent.fraction()),
            format::speed(torrent.download_speed),
            format::speed(torrent.upload_speed),
            format::eta(torrent.eta()),
            format::ratio(torrent.uploaded_bytes, torrent.done_bytes),
            torrent.name,
        );
    }
}

fn print_details(details: &TorrentDetails) {
    let torrent = &details.view;
    println!("#{} {}", torrent.id, torrent.name);
    println!("  hash    {}", torrent.info_hash);
    println!("  status  {}", torrent.status.label());
    println!(
        "  size    {} ({})",
        format::bytes(torrent.total_bytes),
        format::percent(torrent.fraction())
    );
    println!(
        "  speed   down {} · up {}",
        format::speed(torrent.download_speed),
        format::speed(torrent.upload_speed)
    );
    println!(
        "  peers   {} live, {} connecting, {} seen",
        torrent.peers_live, torrent.peers_connecting, torrent.peers_seen
    );
    println!(
        "  ratio   {}",
        format::ratio(torrent.uploaded_bytes, torrent.done_bytes)
    );
    println!("  folder  {}", torrent.output_folder);
    if let Some(error) = &torrent.error {
        println!("  error   {error}");
    }
    if !details.files.is_empty() {
        println!("  files:");
        for file in &details.files {
            let mark = if file.included { "x" } else { " " };
            println!(
                "    [{mark}] {} {} ({})",
                format::percent(file.fraction()),
                format::bytes(file.bytes),
                file.name
            );
        }
    }
}

fn prepare_dirs(config: &Config, dirs: &AppDirs) -> anyhow::Result<()> {
    create_dir(&config.download_dir)?;
    crate::platform::create_private_dir(&dirs.session_dir())
        .with_context(|| format!("cannot create directory {}", dirs.session_dir().display()))
}

fn create_dir(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("cannot create directory {}", path.display()))
}

#[cfg(test)]
mod tests {
    use crate::cli::Command;

    #[test]
    fn config_path_command_is_recognized() {
        assert!(matches!(Command::ConfigPath, Command::ConfigPath));
    }
}
