//! Renders the TUI with sample data and prints the frame as text.
//!
//! ```text
//! cargo run --example preview
//! ```

use ratatui::{Terminal, backend::TestBackend};

use torrx::{
    app::{App, AppEvent},
    domain::{GlobalView, Snapshot, TorrentId, TorrentStatus, TorrentView},
    ui,
};

const WIDTH: u16 = 132;
const HEIGHT: u16 = 24;

fn main() -> anyhow::Result<()> {
    let mut app = App::new("balanced", true, None);

    let torrents = vec![
        torrent(
            1,
            "debian-13.0-amd64-netinst.iso",
            TorrentStatus::Downloading,
            3_700_000_000,
            1_200_000_000,
        ),
        torrent(
            2,
            "ubuntu-26.04-desktop-amd64.iso",
            TorrentStatus::Seeding,
            4_100_000_000,
            4_100_000_000,
        ),
        torrent(
            3,
            "archive-2026-10.tar.zst",
            TorrentStatus::Paused,
            12_000_000_000,
            480_000_000,
        ),
        torrent(
            4,
            "fedora-workstation-44.iso",
            TorrentStatus::Downloading,
            2_300_000_000,
            2_290_000_000,
        ),
    ];

    for step in 0..60 {
        let wave = ((step as u64 * 37) % 7 + 1) * 1024 * 1024;
        app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView {
                download_speed: wave,
                upload_speed: wave / 8,
                ..GlobalView::default()
            },
            torrents: vec![],
        }));
    }

    app.handle_event(AppEvent::Snapshot(Snapshot {
        global: GlobalView {
            download_speed: 8 * 1024 * 1024,
            upload_speed: 786 * 1024,
            downloaded_bytes: 14_800_000_000,
            uploaded_bytes: 5_300_000_000,
            peers_live: 42,
            uptime_seconds: 5_400,
            dht_enabled: true,
            listen_port: Some(51413),
        },
        torrents,
    }));

    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::render(frame, &app))?;

    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            output.push_str(
                buffer
                    .cell((x, y))
                    .map_or(" ", ratatui::buffer::Cell::symbol),
            );
        }
        output.push('\n');
    }
    print!("{output}");
    Ok(())
}

fn torrent(id: usize, name: &str, status: TorrentStatus, total: u64, done: u64) -> TorrentView {
    TorrentView {
        id: TorrentId(id),
        info_hash: format!("{id:040x}"),
        name: name.to_owned(),
        output_folder: "/home/user/downloads".to_owned(),
        total_bytes: total,
        done_bytes: done,
        uploaded_bytes: done / 2,
        download_speed: match status {
            TorrentStatus::Downloading => 3_200_000,
            _ => 0,
        },
        upload_speed: match status {
            TorrentStatus::Seeding => 620_000,
            _ => 0,
        },
        peers_live: 12,
        peers_connecting: 3,
        peers_seen: 48,
        status,
        error: None,
    }
}
