use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::paths::{self, AppDirs};
use crate::platform;

const DEFAULT_PEER_LIMIT: (usize, usize) = (1, 10_000);
const LOG_LEVELS: [&str; 5] = ["trace", "debug", "info", "warn", "error"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lower")]
pub enum Profile {
    #[default]
    Balanced,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub listen: bool,
    pub listen_port: u16,
    pub dht: bool,
    pub lsd: bool,
    pub trackers: bool,
    pub upnp: bool,
    pub seed: bool,
    pub peer_limit: usize,
}

impl Profile {
    pub fn preset(self) -> Preset {
        match self {
            Self::Balanced => Preset {
                listen: true,
                listen_port: 0,
                dht: true,
                lsd: true,
                trackers: true,
                upnp: false,
                seed: true,
                peer_limit: 200,
            },
            Self::Private => Preset {
                listen: true,
                listen_port: 0,
                dht: false,
                lsd: false,
                trackers: true,
                upnp: false,
                seed: false,
                peer_limit: 50,
            },
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Private => "private",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Balanced => "DHT and local peer discovery on, seeding on",
            Self::Private => "tracker-only discovery, DHT/LSD off, seeding off",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FileConfig {
    pub profile: Option<Profile>,
    pub download_dir: Option<String>,
    pub listen: Option<bool>,
    pub listen_port: Option<u16>,
    pub dht: Option<bool>,
    pub lsd: Option<bool>,
    pub trackers: Option<bool>,
    pub upnp: Option<bool>,
    pub seed: Option<bool>,
    pub peer_limit: Option<usize>,
    pub upload_limit_kib: Option<u32>,
    pub download_limit_kib: Option<u32>,
    pub log_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub profile: Profile,
    pub download_dir: PathBuf,
    pub listen: bool,
    pub listen_port: u16,
    pub dht: bool,
    pub lsd: bool,
    pub trackers: bool,
    pub upnp: bool,
    pub seed: bool,
    pub peer_limit: usize,
    pub upload_limit_kib: u32,
    pub download_limit_kib: u32,
    pub log_level: String,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub profile: Option<Profile>,
    pub download_dir: Option<PathBuf>,
    pub verbose: u8,
    pub quiet: bool,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse config file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("peer_limit must be between {low} and {high}, got {value}")]
    PeerLimit {
        value: usize,
        low: usize,
        high: usize,
    },
    #[error("invalid log_level '{0}'")]
    LogLevel(String),
}

pub fn load(
    config_path: Option<&Path>,
    dirs: &AppDirs,
    cli: &CliOverrides,
) -> Result<Config, ConfigError> {
    let (resolved_path, file) = if let Some(path) = config_path {
        (Some(path.to_path_buf()), Some(read_file(path)?))
    } else {
        let path = dirs.config_file();
        let file = if path.exists() {
            Some(read_file(&path)?)
        } else {
            None
        };
        (file.as_ref().map(|_| path), file)
    };

    resolve(
        file,
        cli,
        paths::default_download_dir(),
        paths::home_dir().as_deref(),
        resolved_path,
    )
}

pub fn resolve(
    file: Option<FileConfig>,
    cli: &CliOverrides,
    default_download_dir: PathBuf,
    home: Option<&Path>,
    config_path: Option<PathBuf>,
) -> Result<Config, ConfigError> {
    let file = file.unwrap_or_default();
    let profile = cli.profile.or(file.profile).unwrap_or_default();
    let preset = profile.preset();

    let download_dir = cli
        .download_dir
        .clone()
        .or_else(|| {
            file.download_dir
                .as_deref()
                .map(|dir| platform::expand_tilde(dir, home))
        })
        .unwrap_or(default_download_dir);

    let peer_limit = file.peer_limit.unwrap_or(preset.peer_limit);
    if !(DEFAULT_PEER_LIMIT.0..=DEFAULT_PEER_LIMIT.1).contains(&peer_limit) {
        return Err(ConfigError::PeerLimit {
            value: peer_limit,
            low: DEFAULT_PEER_LIMIT.0,
            high: DEFAULT_PEER_LIMIT.1,
        });
    }

    let log_level = resolve_log_level(file.log_level.as_deref(), cli)?;

    Ok(Config {
        profile,
        download_dir,
        listen: file.listen.unwrap_or(preset.listen),
        listen_port: file.listen_port.unwrap_or(preset.listen_port),
        dht: file.dht.unwrap_or(preset.dht),
        lsd: file.lsd.unwrap_or(preset.lsd),
        trackers: file.trackers.unwrap_or(preset.trackers),
        upnp: file.upnp.unwrap_or(preset.upnp),
        seed: file.seed.unwrap_or(preset.seed),
        peer_limit,
        upload_limit_kib: file.upload_limit_kib.unwrap_or(0),
        download_limit_kib: file.download_limit_kib.unwrap_or(0),
        log_level,
        config_path,
    })
}

fn read_file(path: &Path) -> Result<FileConfig, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&contents).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })
}

fn resolve_log_level(file_level: Option<&str>, cli: &CliOverrides) -> Result<String, ConfigError> {
    let level = if cli.quiet {
        "warn"
    } else {
        match cli.verbose {
            0 => file_level.unwrap_or("info"),
            1 => "debug",
            _ => "trace",
        }
    };
    let normalized = level.trim().to_ascii_lowercase();
    if !LOG_LEVELS.contains(&normalized.as_str()) {
        return Err(ConfigError::LogLevel(level.to_owned()));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn resolve_with(file: FileConfig, cli: &CliOverrides) -> Result<Config, ConfigError> {
        resolve(
            Some(file),
            cli,
            PathBuf::from("/downloads"),
            Some(Path::new("/home/user")),
            None,
        )
    }

    #[test]
    fn balanced_profile_defaults() {
        let config = resolve(
            None,
            &CliOverrides::default(),
            PathBuf::from("/dl"),
            None,
            None,
        )
        .expect("defaults are valid");
        assert_eq!(config.profile, Profile::Balanced);
        assert!(config.dht);
        assert!(config.lsd);
        assert!(config.trackers);
        assert!(!config.upnp);
        assert!(config.seed);
        assert_eq!(config.listen_port, 0);
        assert_eq!(config.download_dir, PathBuf::from("/dl"));
    }

    #[test]
    fn private_profile_disables_discovery_and_seeding() {
        let file = FileConfig {
            profile: Some(Profile::Private),
            ..FileConfig::default()
        };
        let config = resolve_with(file, &CliOverrides::default()).expect("valid config");
        assert!(!config.dht);
        assert!(!config.lsd);
        assert!(config.trackers);
        assert!(!config.seed);
    }

    #[test]
    fn file_keys_override_profile_preset() {
        let file = FileConfig {
            profile: Some(Profile::Private),
            dht: Some(true),
            seed: Some(true),
            peer_limit: Some(10),
            ..FileConfig::default()
        };
        let config = resolve_with(file, &CliOverrides::default()).expect("valid config");
        assert!(config.dht);
        assert!(config.seed);
        assert_eq!(config.peer_limit, 10);
    }

    #[test]
    fn cli_overrides_file_and_preset() {
        let file = FileConfig {
            download_dir: Some("~/torrents".to_owned()),
            profile: Some(Profile::Balanced),
            ..FileConfig::default()
        };
        let cli = CliOverrides {
            profile: Some(Profile::Private),
            download_dir: Some(PathBuf::from("/other")),
            ..CliOverrides::default()
        };
        let config = resolve_with(file, &cli).expect("valid config");
        assert_eq!(config.profile, Profile::Private);
        assert!(!config.dht);
        assert_eq!(config.download_dir, PathBuf::from("/other"));
    }

    #[test]
    fn expands_tilde_in_download_dir() {
        let file = FileConfig {
            download_dir: Some("~/torrents".to_owned()),
            ..FileConfig::default()
        };
        let config = resolve_with(file, &CliOverrides::default()).expect("valid config");
        assert_eq!(config.download_dir, PathBuf::from("/home/user/torrents"));
    }

    #[test]
    fn rejects_invalid_peer_limit() {
        let file = FileConfig {
            peer_limit: Some(0),
            ..FileConfig::default()
        };
        assert!(matches!(
            resolve_with(file, &CliOverrides::default()),
            Err(ConfigError::PeerLimit { value: 0, .. })
        ));
    }

    #[test]
    fn rejects_invalid_log_level() {
        let file = FileConfig {
            log_level: Some("not a level".to_owned()),
            ..FileConfig::default()
        };
        assert!(matches!(
            resolve_with(file, &CliOverrides::default()),
            Err(ConfigError::LogLevel(_))
        ));
    }

    #[test]
    fn normalizes_log_level_case() {
        let file = FileConfig {
            log_level: Some("DEBUG".to_owned()),
            ..FileConfig::default()
        };
        let config = resolve_with(file, &CliOverrides::default()).expect("valid config");
        assert_eq!(config.log_level, "debug");
    }

    #[test]
    fn verbose_and_quiet_flags_win_over_file_level() {
        let file = FileConfig {
            log_level: Some("info".to_owned()),
            ..FileConfig::default()
        };
        let debug = resolve_with(
            file.clone(),
            &CliOverrides {
                verbose: 1,
                ..CliOverrides::default()
            },
        )
        .expect("valid config");
        assert_eq!(debug.log_level, "debug");

        let quiet = resolve_with(
            file,
            &CliOverrides {
                verbose: 2,
                quiet: true,
                ..CliOverrides::default()
            },
        )
        .expect("valid config");
        assert_eq!(quiet.log_level, "warn");
    }

    #[test]
    fn rejects_unknown_keys_in_file() {
        let parsed: Result<FileConfig, _> = toml::from_str("not_a_key = 1");
        assert!(parsed.is_err());
    }

    #[test]
    fn parses_profiles_from_toml_case_insensitively() {
        let parsed: FileConfig = toml::from_str("profile = \"private\"").expect("valid toml");
        assert_eq!(parsed.profile, Some(Profile::Private));
    }
}
