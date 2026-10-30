//! End-to-end test: a seeder and a leecher session exchange a real payload
//! over loopback with DHT, LSD and trackers disabled, so the transfer is
//! deterministic and fully offline.

use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
    time::{Duration, Instant},
};

use tempfile::TempDir;
use torrx::{
    config::{Config, Profile},
    domain::{TorrentStatus, TorrentView},
    engine::{AddOptions, AddSource, LibrqbitEngine, TorrentEngine},
};

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const TIMEOUT: Duration = Duration::from_secs(60);
const PAYLOAD_SIZE: usize = 2 * 1024 * 1024 + 1234;

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

async fn open_engine(dir: &Path) -> anyhow::Result<LibrqbitEngine> {
    LibrqbitEngine::open(
        &offline_config(dir),
        &dir.join("session-state"),
        &dir.join("dht-state.json"),
    )
    .await
}

fn payload_bytes(size: usize) -> Vec<u8> {
    (0..size).map(|index| (index % 251) as u8).collect()
}

async fn wait_until<F>(engine: &dyn TorrentEngine, mut predicate: F, what: &str)
where
    F: FnMut(&TorrentView) -> bool,
{
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let snapshot = engine.snapshot().await;
        if let Some(view) = snapshot.torrents.first()
            && predicate(view)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {TIMEOUT:?} waiting for {what}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[tokio::test]
async fn seeds_and_downloads_a_real_payload_over_loopback() {
    let root = TempDir::new().expect("temp dir");
    let seed_dir = root.path().join("seed");
    let leech_dir = root.path().join("leech");
    std::fs::create_dir_all(&seed_dir).expect("seed dir");
    std::fs::create_dir_all(&leech_dir).expect("leech dir");

    let payload = payload_bytes(PAYLOAD_SIZE);
    let payload_path = seed_dir.join("payload.bin");
    std::fs::write(&payload_path, &payload).expect("writes payload");

    let created = librqbit::create_torrent(
        &payload_path,
        librqbit::CreateTorrentOptions {
            name: None,
            trackers: vec![],
            piece_length: Some(64 * 1024),
        },
        &librqbit::spawn_utils::BlockingSpawner::new(1),
    )
    .await
    .expect("creates torrent");

    let torrent_file = root.path().join("payload.torrent");
    std::fs::write(&torrent_file, created.as_bytes().expect("torrent bytes"))
        .expect("writes torrent file");

    let seeder = open_engine(&seed_dir).await.expect("seeder engine starts");
    let outcome = seeder
        .add(AddSource::File(torrent_file.clone()), AddOptions::default())
        .await
        .expect("seeder adds torrent");
    assert_eq!(outcome.name, "payload.bin");

    wait_until(
        &seeder,
        |view| view.status == TorrentStatus::Seeding,
        "seeder to check and offer the payload",
    )
    .await;

    let port = seeder
        .snapshot()
        .await
        .global
        .listen_port
        .expect("seeder is listening");

    let leecher = open_engine(&leech_dir)
        .await
        .expect("leecher engine starts");
    leecher
        .add(
            AddSource::File(torrent_file.clone()),
            AddOptions {
                initial_peers: vec![
                    SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                    SocketAddr::from((Ipv6Addr::LOCALHOST, port)),
                ],
                ..AddOptions::default()
            },
        )
        .await
        .expect("leecher adds torrent");

    wait_until(
        &leecher,
        |view| view.status == TorrentStatus::Seeding,
        "leecher to finish the download",
    )
    .await;

    let downloaded =
        std::fs::read(leech_dir.join("payload.bin")).expect("downloaded payload exists");
    assert_eq!(downloaded, payload, "downloaded bytes match the payload");

    let leech_view = leecher.snapshot().await.torrents.remove(0);
    assert_eq!(leech_view.done_bytes, PAYLOAD_SIZE as u64);
    assert_eq!(leech_view.fraction(), 1.0);
    assert_eq!(leech_view.info_hash, outcome.info_hash);

    let seed_view = seeder.snapshot().await.torrents.remove(0);
    assert!(
        seed_view.uploaded_bytes > 0,
        "seeder uploaded {PAYLOAD_SIZE} bytes of payload"
    );

    // A third session downloads exclusively from the first leecher, which
    // proves completed sessions keep serving data (upload path).
    let second_leech_dir = root.path().join("leech-2");
    std::fs::create_dir_all(&second_leech_dir).expect("second leech dir");
    let leech_port = leecher
        .snapshot()
        .await
        .global
        .listen_port
        .expect("leecher is listening");

    let second_leecher = open_engine(&second_leech_dir)
        .await
        .expect("second leecher engine starts");
    second_leecher
        .add(
            AddSource::File(torrent_file.clone()),
            AddOptions {
                initial_peers: vec![
                    SocketAddr::from((Ipv4Addr::LOCALHOST, leech_port)),
                    SocketAddr::from((Ipv6Addr::LOCALHOST, leech_port)),
                ],
                ..AddOptions::default()
            },
        )
        .await
        .expect("second leecher adds torrent");

    wait_until(
        &second_leecher,
        |view| view.status == TorrentStatus::Seeding,
        "second leecher to finish the download",
    )
    .await;

    let second_payload =
        std::fs::read(second_leech_dir.join("payload.bin")).expect("second payload exists");
    assert_eq!(second_payload, payload);

    let leech_view = leecher.snapshot().await.torrents.remove(0);
    assert!(
        leech_view.uploaded_bytes > 0,
        "first leecher served data to the second one"
    );

    seeder.shutdown().await;
    leecher.shutdown().await;
    second_leecher.shutdown().await;
}
