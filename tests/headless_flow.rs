//! Headless flow: add a paused torrent from a `.torrent` file, inspect it,
//! resume/pause and remove it, without the TUI.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use tempfile::TempDir;
use torrx::{
    config::{Config, Profile},
    domain::TorrentStatus,
    engine::{AddOptions, AddSource, LibrqbitEngine, TorrentEngine},
};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const TIMEOUT: Duration = Duration::from_secs(30);

fn offline_config(download_dir: &Path) -> Config {
    Config {
        profile: Profile::Private,
        download_dir: download_dir.to_path_buf(),
        listen: true,
        listen_port: 0,
        dht: false,
        lsd: false,
        trackers: false,
        upnp: false,
        seed: true,
        peer_limit: 20,
        upload_limit_kib: 0,
        download_limit_kib: 0,
        log_level: "info".to_owned(),
        config_path: None,
    }
}

async fn wait_until_paused(engine: &LibrqbitEngine) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let snapshot = engine.snapshot().await;
        if snapshot
            .torrents
            .first()
            .is_some_and(|view| view.status == TorrentStatus::Paused)
        {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for paused");
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[tokio::test]
async fn headless_add_inspect_resume_pause_and_remove() {
    let root = TempDir::new().expect("temp dir");
    let seed_dir = root.path().join("seed");
    let download_dir = root.path().join("download");
    std::fs::create_dir_all(&seed_dir).expect("seed dir");
    std::fs::create_dir_all(&download_dir).expect("download dir");

    let payload = vec![7_u8; 512 * 1024];
    let payload_path = seed_dir.join("payload.bin");
    std::fs::write(&payload_path, &payload).expect("writes payload");

    let created = librqbit::create_torrent(
        &payload_path,
        librqbit::CreateTorrentOptions {
            name: None,
            trackers: vec![],
            piece_length: Some(32 * 1024),
        },
        &librqbit::spawn_utils::BlockingSpawner::new(1),
    )
    .await
    .expect("creates torrent");
    let torrent_path = root.path().join("payload.torrent");
    std::fs::write(&torrent_path, created.as_bytes().expect("torrent bytes"))
        .expect("writes torrent");

    let engine = LibrqbitEngine::open(
        &offline_config(&download_dir),
        &root.path().join("state"),
        &root.path().join("dht.json"),
    )
    .await
    .expect("opens session");

    let source =
        AddSource::parse(torrent_path.to_str().expect("utf-8 path"), None).expect("parses source");
    let outcome = engine
        .add(
            source,
            AddOptions {
                paused: true,
                ..AddOptions::default()
            },
        )
        .await
        .expect("adds paused torrent");
    assert_eq!(outcome.name, "payload.bin");

    wait_until_paused(&engine).await;

    let snapshot = engine.snapshot().await;
    assert_eq!(snapshot.torrents.len(), 1);
    assert_eq!(snapshot.torrents[0].total_bytes, payload.len() as u64);

    let details = engine.details(outcome.id).await.expect("loads details");
    assert_eq!(details.files.len(), 1);
    assert_eq!(details.files[0].name, "payload.bin");
    assert_eq!(details.files[0].bytes, payload.len() as u64);
    assert!(details.files[0].included);

    engine.resume(outcome.id).await.expect("resumes");
    engine.pause(outcome.id).await.expect("pauses");
    wait_until_paused(&engine).await;

    engine
        .remove(outcome.id, false)
        .await
        .expect("removes torrent");
    assert_eq!(engine.snapshot().await.torrents, Vec::new());
    engine.shutdown().await;
}
