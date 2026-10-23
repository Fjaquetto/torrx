use crate::domain::{TorrentStatus, TorrentView};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawState {
    Initializing { paused: bool },
    Live,
    Paused,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RawPeers {
    pub live: u32,
    pub connecting: u32,
    pub queued: u32,
    pub seen: u32,
}

#[derive(Debug, Clone)]
pub struct RawTorrent {
    pub id: usize,
    pub info_hash: String,
    pub name: String,
    pub output_folder: String,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub uploaded_bytes: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub finished: bool,
    pub state: RawState,
    pub peers: RawPeers,
    pub error: Option<String>,
}

pub fn map_torrent(raw: RawTorrent) -> TorrentView {
    TorrentView {
        id: crate::domain::TorrentId(raw.id),
        info_hash: raw.info_hash,
        name: raw.name,
        output_folder: raw.output_folder,
        total_bytes: raw.total_bytes,
        done_bytes: raw.done_bytes,
        uploaded_bytes: raw.uploaded_bytes,
        download_speed: raw.download_speed,
        upload_speed: raw.upload_speed,
        peers_live: raw.peers.live,
        peers_connecting: raw.peers.connecting,
        peers_seen: raw.peers.seen,
        status: map_status(&raw.state, raw.finished),
        error: raw.error,
    }
}

pub fn map_status(state: &RawState, finished: bool) -> TorrentStatus {
    match state {
        RawState::Error => TorrentStatus::Error,
        RawState::Initializing { paused: true } | RawState::Paused => TorrentStatus::Paused,
        RawState::Initializing { paused: false } => TorrentStatus::Initializing,
        RawState::Live if finished => TorrentStatus::Seeding,
        RawState::Live => TorrentStatus::Downloading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(state: RawState, finished: bool) -> RawTorrent {
        RawTorrent {
            id: 7,
            info_hash: "aa".to_owned(),
            name: "debian".to_owned(),
            output_folder: "/dl".to_owned(),
            total_bytes: 1000,
            done_bytes: 500,
            uploaded_bytes: 10,
            download_speed: 100,
            upload_speed: 5,
            finished,
            state,
            peers: RawPeers {
                live: 3,
                connecting: 1,
                queued: 0,
                seen: 12,
            },
            error: None,
        }
    }

    #[test]
    fn maps_live_states_by_completion() {
        assert_eq!(
            map_status(&RawState::Live, false),
            TorrentStatus::Downloading
        );
        assert_eq!(map_status(&RawState::Live, true), TorrentStatus::Seeding);
    }

    #[test]
    fn maps_paused_and_error_states() {
        assert_eq!(map_status(&RawState::Paused, false), TorrentStatus::Paused);
        assert_eq!(
            map_status(&RawState::Initializing { paused: true }, false),
            TorrentStatus::Paused
        );
        assert_eq!(
            map_status(&RawState::Initializing { paused: false }, false),
            TorrentStatus::Initializing
        );
        assert_eq!(map_status(&RawState::Error, true), TorrentStatus::Error);
    }

    #[test]
    fn carries_fields_into_view() {
        let view = map_torrent(raw(RawState::Live, false));
        assert_eq!(view.id, crate::domain::TorrentId(7));
        assert_eq!(view.name, "debian");
        assert_eq!(view.done_bytes, 500);
        assert_eq!(view.peers_live, 3);
        assert_eq!(view.peers_seen, 12);
        assert_eq!(view.status, TorrentStatus::Downloading);
    }
}
