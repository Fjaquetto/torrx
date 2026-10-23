use std::{
    collections::HashSet,
    net::{IpAddr, Ipv6Addr, SocketAddr},
    num::NonZeroU32,
    path::Path,
    sync::Arc,
};

use anyhow::Context;
use async_trait::async_trait;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListenerOptions,
    ManagedTorrent, Session, SessionOptions, SessionPersistenceConfig, TorrentStatsState,
    api::TorrentIdOrHash, dht::DhtPersistenceConfig, limits::LimitsConfig,
};

use super::{
    AddOptions, AddOutcome, AddSource, EngineError, TorrentEngine,
    mapping::{self, RawPeers, RawState, RawTorrent},
};
use crate::{
    config::Config,
    domain::{FileView, GlobalView, SelectorError, Snapshot, TorrentDetails, TorrentId},
};

pub struct LibrqbitEngine {
    session: Arc<Session>,
}

impl LibrqbitEngine {
    pub async fn open(
        config: &Config,
        session_dir: &Path,
        dht_file: &Path,
    ) -> anyhow::Result<Self> {
        let options = SessionOptions {
            dht: config.dht.then(|| DhtSessionConfig {
                bootstrap_addrs: None,
                port: None,
                persistence: Some(DhtPersistenceConfig {
                    dump_interval: None,
                    config_filename: Some(dht_file.to_path_buf()),
                }),
            }),
            disable_trackers: !config.trackers,
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(session_dir.to_path_buf()),
            }),
            listen: config.listen.then(|| ListenerOptions {
                listen_addr: SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), config.listen_port),
                enable_upnp_port_forwarding: config.upnp,
                ..ListenerOptions::default()
            }),
            ratelimits: LimitsConfig {
                upload_bps: NonZeroU32::new(config.upload_limit_kib.saturating_mul(1024)),
                download_bps: NonZeroU32::new(config.download_limit_kib.saturating_mul(1024)),
            },
            peer_limit: Some(config.peer_limit),
            disable_local_service_discovery: !config.lsd,
            client_name_and_version: Some(format!("torrx {}", env!("CARGO_PKG_VERSION"))),
            ..SessionOptions::default()
        };

        let session = Session::new_with_opts(config.download_dir.clone(), options)
            .await
            .context("cannot start torrent session")?;

        Ok(Self { session })
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    fn handle(&self, id: TorrentId) -> Result<Arc<ManagedTorrent>, EngineError> {
        self.session
            .get(TorrentIdOrHash::Id(id.0))
            .ok_or_else(|| EngineError::Selector(SelectorError::NotFound(id.to_string())))
    }
}

#[async_trait]
impl TorrentEngine for LibrqbitEngine {
    async fn snapshot(&self) -> Snapshot {
        let stats = self.session.stats_snapshot();
        let global = GlobalView {
            download_speed: stats.download_speed.as_bytes(),
            upload_speed: stats.upload_speed.as_bytes(),
            downloaded_bytes: stats.counters.fetched_bytes,
            uploaded_bytes: stats.counters.uploaded_bytes,
            peers_live: stats.peers.live,
            uptime_seconds: stats.uptime_seconds,
            dht_enabled: self.session.get_dht().is_some(),
            listen_port: self.session.listen_addr().map(|addr| addr.port()),
        };

        let torrents = self.session.with_torrents(|iter| {
            iter.map(|(_, handle)| mapping::map_torrent(raw_torrent(handle)))
                .collect()
        });

        Snapshot { global, torrents }
    }

    async fn details(&self, id: TorrentId) -> Result<TorrentDetails, EngineError> {
        let handle = self.handle(id)?;
        let view = mapping::map_torrent(raw_torrent(&handle));
        let stats = handle.stats();
        let file_infos = handle
            .with_metadata(|metadata| metadata.file_infos.clone())
            .unwrap_or_default();
        let only_files = handle.only_files();

        let files = file_infos
            .iter()
            .enumerate()
            .map(|(index, info)| FileView {
                index,
                name: info.relative_filename.display().to_string(),
                bytes: info.len,
                done_bytes: stats.file_progress.get(index).copied().unwrap_or(0),
                included: only_files
                    .as_ref()
                    .is_none_or(|included| included.contains(&index)),
            })
            .collect();

        Ok(TorrentDetails { view, files })
    }

    async fn add(&self, source: AddSource, options: AddOptions) -> Result<AddOutcome, EngineError> {
        let torrent = match &source {
            AddSource::Magnet(value) | AddSource::Url(value) => AddTorrent::from_url(value.clone()),
            AddSource::File(path) => AddTorrent::from_local_filename(&path_to_string(path)?)
                .map_err(|error| failed(&error))?,
        };

        let output_folder = options
            .output_dir
            .as_deref()
            .map(path_to_string)
            .transpose()?;
        let add_options = AddTorrentOptions {
            paused: options.paused,
            overwrite: true,
            output_folder,
            initial_peers: (!options.initial_peers.is_empty()).then_some(options.initial_peers),
            ..AddTorrentOptions::default()
        };

        let response = self
            .session
            .clone()
            .add_torrent(torrent, Some(add_options))
            .await
            .map_err(|error| failed(&error))?;

        match response {
            AddTorrentResponse::Added(id, handle)
            | AddTorrentResponse::AlreadyManaged(id, handle) => Ok(AddOutcome {
                id: TorrentId(id),
                name: handle.name().unwrap_or_else(|| format!("torrent-{id}")),
                info_hash: handle.info_hash().as_string(),
            }),
            AddTorrentResponse::ListOnly(_) => Err(EngineError::Failed(
                "session returned a list-only response".to_owned(),
            )),
        }
    }

    async fn pause(&self, id: TorrentId) -> Result<(), EngineError> {
        let handle = self.handle(id)?;
        self.session
            .pause(&handle)
            .await
            .map_err(|error| failed(&error))
    }

    async fn resume(&self, id: TorrentId) -> Result<(), EngineError> {
        let handle = self.handle(id)?;
        self.session
            .clone()
            .unpause(&handle)
            .await
            .map_err(|error| failed(&error))
    }

    async fn remove(&self, id: TorrentId, delete_files: bool) -> Result<(), EngineError> {
        self.session
            .delete(TorrentIdOrHash::Id(id.0), delete_files)
            .await
            .map_err(|error| failed(&error))
    }

    async fn set_included_files(
        &self,
        id: TorrentId,
        included: Vec<usize>,
    ) -> Result<(), EngineError> {
        let handle = self.handle(id)?;
        let included: HashSet<usize> = included.into_iter().collect();
        self.session
            .clone()
            .update_only_files(&handle, &included)
            .await
            .map_err(|error| failed(&error))
    }

    async fn shutdown(&self) {
        self.session.stop().await;
    }
}

fn raw_torrent(handle: &ManagedTorrent) -> RawTorrent {
    let stats = handle.stats();
    let state = match stats.state {
        TorrentStatsState::Initializing { paused } => RawState::Initializing { paused },
        TorrentStatsState::Live => RawState::Live,
        TorrentStatsState::Paused => RawState::Paused,
        TorrentStatsState::Error => RawState::Error,
    };

    let (download_speed, upload_speed, peers) = match &stats.live {
        Some(live) => {
            let aggregate = &live.snapshot.peer_stats;
            (
                live.download_speed.as_bytes(),
                live.upload_speed.as_bytes(),
                RawPeers {
                    live: aggregate.live,
                    connecting: aggregate.connecting,
                    queued: aggregate.queued,
                    seen: aggregate.seen,
                },
            )
        }
        None => (0, 0, RawPeers::default()),
    };

    RawTorrent {
        id: handle.id(),
        info_hash: handle.info_hash().as_string(),
        name: handle
            .name()
            .unwrap_or_else(|| format!("torrent-{}", handle.id())),
        output_folder: handle.output_folder().display().to_string(),
        total_bytes: stats.total_bytes,
        done_bytes: stats.progress_bytes,
        uploaded_bytes: stats.uploaded_bytes,
        download_speed,
        upload_speed,
        finished: stats.finished,
        state,
        peers,
        error: stats.error,
    }
}

fn path_to_string(path: &Path) -> Result<String, EngineError> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| EngineError::NonUtf8Path(path.display().to_string()))
}

fn failed(error: &anyhow::Error) -> EngineError {
    EngineError::Failed(format!("{error:#}"))
}
