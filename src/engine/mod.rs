pub mod librqbit_engine;
pub mod mapping;

#[cfg(test)]
pub mod fake;

pub use librqbit_engine::LibrqbitEngine;

use std::{net::SocketAddr, path::PathBuf};

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{SelectorError, Snapshot, TorrentDetails, TorrentId, resolve_selector};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error(transparent)]
    Selector(#[from] SelectorError),
    #[error("{0}")]
    Failed(String),
    #[error("path is not valid UTF-8: {0}")]
    NonUtf8Path(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddSource {
    Magnet(String),
    Url(String),
    File(PathBuf),
}

impl AddSource {
    pub fn classify(input: &str) -> Self {
        let value = input.trim();
        if value.starts_with("magnet:") {
            Self::Magnet(value.to_owned())
        } else if value.starts_with("http://") || value.starts_with("https://") {
            Self::Url(value.to_owned())
        } else {
            Self::File(PathBuf::from(value))
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Magnet(value) | Self::Url(value) => value.clone(),
            Self::File(path) => path.display().to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    pub paused: bool,
    pub output_dir: Option<PathBuf>,
    pub initial_peers: Vec<SocketAddr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddOutcome {
    pub id: TorrentId,
    pub name: String,
    pub info_hash: String,
}

#[async_trait]
pub trait TorrentEngine: Send + Sync {
    async fn snapshot(&self) -> Snapshot;
    async fn details(&self, id: TorrentId) -> Result<TorrentDetails, EngineError>;
    async fn add(&self, source: AddSource, options: AddOptions) -> Result<AddOutcome, EngineError>;
    async fn pause(&self, id: TorrentId) -> Result<(), EngineError>;
    async fn resume(&self, id: TorrentId) -> Result<(), EngineError>;
    async fn remove(&self, id: TorrentId, delete_files: bool) -> Result<(), EngineError>;
    async fn set_included_files(
        &self,
        id: TorrentId,
        included: Vec<usize>,
    ) -> Result<(), EngineError>;
    async fn shutdown(&self);

    async fn resolve(&self, selector: &str) -> Result<TorrentId, EngineError> {
        let snapshot = self.snapshot().await;
        let candidates: Vec<(TorrentId, String)> = snapshot
            .torrents
            .into_iter()
            .map(|torrent| (torrent.id, torrent.info_hash))
            .collect();
        Ok(resolve_selector(selector, &candidates)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_magnet_links() {
        assert_eq!(
            AddSource::classify(" magnet:?xt=urn:btih:aa "),
            AddSource::Magnet("magnet:?xt=urn:btih:aa".to_owned())
        );
    }

    #[test]
    fn classifies_http_sources() {
        assert!(matches!(
            AddSource::classify("https://example.org/file.torrent"),
            AddSource::Url(_)
        ));
    }

    #[test]
    fn classifies_files() {
        assert_eq!(
            AddSource::classify("./debian.torrent"),
            AddSource::File(PathBuf::from("./debian.torrent"))
        );
    }

    #[tokio::test]
    async fn resolves_selectors_through_any_engine() {
        use crate::domain::{GlobalView, Snapshot, TorrentStatus, TorrentView};

        let engine = fake::FakeEngine::default();
        engine.set_snapshot(Snapshot {
            global: GlobalView::default(),
            torrents: vec![TorrentView {
                id: TorrentId(3),
                info_hash: "aabbccdd".to_owned(),
                name: "test".to_owned(),
                output_folder: "/tmp".to_owned(),
                total_bytes: 1,
                done_bytes: 0,
                uploaded_bytes: 0,
                download_speed: 0,
                upload_speed: 0,
                peers_live: 0,
                peers_connecting: 0,
                peers_seen: 0,
                status: TorrentStatus::Downloading,
                error: None,
            }],
        });

        assert_eq!(engine.resolve("3").await, Ok(TorrentId(3)));
        assert_eq!(engine.resolve("AABB").await, Ok(TorrentId(3)));
        assert!(engine.resolve("ffff").await.is_err());
    }
}
