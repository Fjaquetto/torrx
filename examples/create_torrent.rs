//! Creates a .torrent file for a local file or directory.
//!
//! Useful to try torrx against your own payloads:
//!
//! ```text
//! cargo run --example create_torrent -- ./payload.bin payload.torrent
//! ```

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(about = "Create a .torrent file for a local file or directory")]
struct Args {
    #[arg(value_name = "PATH")]
    path: PathBuf,
    #[arg(value_name = "OUTPUT")]
    output: PathBuf,
    #[arg(
        long,
        value_name = "ANNOUNCE",
        help = "Tracker announce URL (repeatable)"
    )]
    tracker: Vec<String>,
    #[arg(long, default_value_t = 262_144, help = "Piece length in bytes")]
    piece_length: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let result = librqbit::create_torrent(
        &args.path,
        librqbit::CreateTorrentOptions {
            name: None,
            trackers: args.tracker,
            piece_length: Some(args.piece_length),
        },
        &librqbit::spawn_utils::BlockingSpawner::new(1),
    )
    .await?;

    std::fs::write(&args.output, result.as_bytes()?)?;
    println!(
        "wrote {} (info hash {})",
        args.output.display(),
        result.info_hash().as_string()
    );
    Ok(())
}
