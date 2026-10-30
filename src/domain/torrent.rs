use std::{fmt, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TorrentId(pub usize);

impl fmt::Display for TorrentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorrentStatus {
    Initializing,
    Downloading,
    Seeding,
    Paused,
    Error,
}

impl TorrentStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Initializing => "init",
            Self::Downloading => "down",
            Self::Seeding => "seed",
            Self::Paused => "pause",
            Self::Error => "error",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Downloading | Self::Seeding)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TorrentView {
    pub id: TorrentId,
    pub info_hash: String,
    pub name: String,
    pub output_folder: String,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub peers_live: u32,
    pub peers_connecting: u32,
    pub peers_seen: u32,
    pub status: TorrentStatus,
    pub error: Option<String>,
}

impl TorrentView {
    pub fn fraction(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.done_bytes as f64 / self.total_bytes as f64
    }

    pub fn remaining_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.done_bytes)
    }

    pub fn eta(&self) -> Option<Duration> {
        let remaining = self.remaining_bytes();
        if remaining == 0 || self.download_speed == 0 {
            return None;
        }
        Some(Duration::from_secs(remaining / self.download_speed))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileView {
    pub index: usize,
    pub name: String,
    pub bytes: u64,
    pub done_bytes: u64,
    pub included: bool,
}

impl FileView {
    pub fn fraction(&self) -> f64 {
        if self.bytes == 0 {
            return 0.0;
        }
        self.done_bytes as f64 / self.bytes as f64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TorrentDetails {
    pub view: TorrentView,
    pub files: Vec<FileView>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalView {
    pub download_speed: u64,
    pub upload_speed: u64,
    pub downloaded_bytes: u64,
    pub uploaded_bytes: u64,
    pub peers_live: u32,
    pub uptime_seconds: u64,
    pub dht_enabled: bool,
    pub listen_port: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub global: GlobalView,
    pub torrents: Vec<TorrentView>,
}

impl Snapshot {
    pub fn find(&self, id: TorrentId) -> Option<&TorrentView> {
        self.torrents.iter().find(|t| t.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(total: u64, done: u64, speed: u64) -> TorrentView {
        TorrentView {
            id: TorrentId(1),
            info_hash: "aa".to_owned(),
            name: "test".to_owned(),
            output_folder: "/tmp".to_owned(),
            total_bytes: total,
            done_bytes: done,
            uploaded_bytes: 0,
            download_speed: speed,
            upload_speed: 0,
            peers_live: 0,
            peers_connecting: 0,
            peers_seen: 0,
            status: TorrentStatus::Downloading,
            error: None,
        }
    }

    #[test]
    fn fraction_handles_empty_torrent() {
        assert_eq!(view(0, 0, 0).fraction(), 0.0);
    }

    #[test]
    fn fraction_divides_done_by_total() {
        assert_eq!(view(1000, 250, 0).fraction(), 0.25);
    }

    #[test]
    fn eta_is_none_without_speed_or_remaining() {
        assert_eq!(view(1000, 500, 0).eta(), None);
        assert_eq!(view(1000, 1000, 100).eta(), None);
    }

    #[test]
    fn eta_divides_remaining_by_speed() {
        assert_eq!(view(1000, 0, 100).eta(), Some(Duration::from_secs(10)));
    }

    #[test]
    fn status_labels_are_short() {
        assert_eq!(TorrentStatus::Downloading.label(), "down");
        assert!(TorrentStatus::Seeding.is_active());
        assert!(!TorrentStatus::Paused.is_active());
    }

    #[test]
    fn torrent_id_formatting_honors_padding() {
        assert_eq!(format!("{:>4}", TorrentId(7)), "   7");
        assert_eq!(TorrentId(12).to_string(), "12");
    }

    #[test]
    fn snapshot_finds_torrent_by_id() {
        let snapshot = Snapshot {
            global: GlobalView::default(),
            torrents: vec![view(1, 1, 0)],
        };
        assert!(snapshot.find(TorrentId(1)).is_some());
        assert!(snapshot.find(TorrentId(2)).is_none());
    }
}
