pub mod librqbit_engine;
pub mod mapping;

#[cfg(test)]
pub mod fake;

pub use librqbit_engine::LibrqbitEngine;

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use thiserror::Error;
use url::Url;

use crate::domain::{SelectorError, Snapshot, TorrentDetails, TorrentId, resolve_selector};
use crate::platform;

const MAX_SOURCE_LEN: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error(transparent)]
    Selector(#[from] SelectorError),
    #[error("{0}")]
    Failed(String),
    #[error("path is not valid UTF-8: {0}")]
    NonUtf8Path(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SourceError {
    #[error("source is empty")]
    Empty,
    #[error("source is too long (max {MAX_SOURCE_LEN} characters)")]
    TooLong,
    #[error("source contains control characters")]
    ControlCharacters,
    #[error("invalid file URL: {0}")]
    InvalidFileUrl(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddSource {
    Magnet(String),
    Url(String),
    File(PathBuf),
}

impl AddSource {
    pub fn parse(input: &str, home: Option<&Path>) -> Result<Self, SourceError> {
        let value = strip_wrapping_quotes(input.trim()).trim();

        if value.is_empty() {
            return Err(SourceError::Empty);
        }
        if value.chars().count() > MAX_SOURCE_LEN {
            return Err(SourceError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(SourceError::ControlCharacters);
        }
        if value.starts_with("magnet:") {
            return Ok(Self::Magnet(value.to_owned()));
        }
        if value.starts_with("file://") {
            let url = Url::parse(value)
                .map_err(|error| SourceError::InvalidFileUrl(error.to_string()))?;
            return Ok(Self::File(file_url_to_path(&url)?));
        }
        if value.starts_with("http://") || value.starts_with("https://") {
            return Ok(Self::Url(value.to_owned()));
        }
        Ok(Self::File(platform::expand_tilde(value, home)))
    }

    pub fn display(&self) -> String {
        match self {
            Self::Magnet(value) | Self::Url(value) => value.clone(),
            Self::File(path) => path.display().to_string(),
        }
    }
}

fn strip_wrapping_quotes(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

fn file_url_to_path(url: &Url) -> Result<PathBuf, SourceError> {
    let host = url.host_str().unwrap_or_default();
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return Err(SourceError::InvalidFileUrl(format!(
            "remote host '{host}' is not supported"
        )));
    }
    if !is_local_file_path(url.path()) {
        return Err(SourceError::InvalidFileUrl(
            "expected an absolute file:///path".to_owned(),
        ));
    }
    url.to_file_path()
        .map_err(|()| SourceError::InvalidFileUrl("not a local file path".to_owned()))
}

fn is_local_file_path(path: &str) -> bool {
    if cfg!(windows) {
        is_windows_local_file_path(path)
    } else {
        path.starts_with('/')
    }
}

fn is_windows_local_file_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.len() >= 4
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && bytes[3] == b'/'
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

    fn parse(input: &str) -> Result<AddSource, SourceError> {
        AddSource::parse(input, Some(Path::new("/home/user")))
    }

    #[test]
    fn parses_magnet_links() {
        assert_eq!(
            parse(" magnet:?xt=urn:btih:aa "),
            Ok(AddSource::Magnet("magnet:?xt=urn:btih:aa".to_owned()))
        );
    }

    #[test]
    fn parses_http_sources() {
        assert_eq!(
            parse("https://example.org/file.torrent"),
            Ok(AddSource::Url(
                "https://example.org/file.torrent".to_owned()
            ))
        );
    }

    #[test]
    fn parses_files() {
        assert_eq!(
            parse("./debian.torrent"),
            Ok(AddSource::File(PathBuf::from("./debian.torrent")))
        );
    }

    #[test]
    fn expands_tilde_and_strips_quotes() {
        assert_eq!(
            parse("\"~/Downloads/debian.torrent\""),
            Ok(AddSource::File(PathBuf::from(
                "/home/user/Downloads/debian.torrent"
            )))
        );
        assert_eq!(
            parse("'/tmp/a b.torrent'"),
            Ok(AddSource::File(PathBuf::from("/tmp/a b.torrent")))
        );
    }

    #[test]
    fn parses_file_urls_with_percent_encoding() {
        let path = if cfg!(windows) {
            PathBuf::from(r"C:\torrx tests\My File.torrent")
        } else {
            PathBuf::from("/tmp/My File.torrent")
        };
        let url = Url::from_file_path(&path).expect("absolute path becomes a file URL");
        assert_eq!(
            AddSource::parse(url.as_str(), None),
            Ok(AddSource::File(path))
        );
    }

    #[test]
    fn rejects_empty_and_control_characters() {
        assert_eq!(parse("   "), Err(SourceError::Empty));
        assert_eq!(
            parse("magnet:?xt=aa\nbb"),
            Err(SourceError::ControlCharacters)
        );
    }

    #[test]
    fn rejects_oversized_sources() {
        let huge = "a".repeat(MAX_SOURCE_LEN + 1);
        assert_eq!(parse(&huge), Err(SourceError::TooLong));
    }

    #[test]
    fn rejects_invalid_file_urls() {
        assert!(matches!(
            parse("file://relative"),
            Err(SourceError::InvalidFileUrl(_))
        ));
        assert!(matches!(
            parse("file://example.com/share/file.torrent"),
            Err(SourceError::InvalidFileUrl(_))
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rejects_unix_style_file_urls_on_windows() {
        assert!(matches!(
            parse("file:///tmp/My%20File.torrent"),
            Err(SourceError::InvalidFileUrl(_))
        ));
    }

    #[test]
    fn windows_file_paths_require_a_drive_letter() {
        assert!(is_windows_local_file_path("/C:/Users/file.torrent"));
        assert!(is_windows_local_file_path("/d:/data/file.torrent"));
        assert!(!is_windows_local_file_path("/tmp/file.torrent"));
        assert!(!is_windows_local_file_path("/C:"));
        assert!(!is_windows_local_file_path("C:/file.torrent"));
        assert!(!is_windows_local_file_path("/1:/file.torrent"));
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
