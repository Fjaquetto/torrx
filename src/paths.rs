use std::path::{Path, PathBuf};

use directories::{ProjectDirs, UserDirs};

#[derive(Debug, Clone)]
pub struct AppDirs {
    config: PathBuf,
    state: PathBuf,
}

impl AppDirs {
    pub fn new(config: PathBuf, state: PathBuf) -> Self {
        Self { config, state }
    }

    pub fn discover() -> Option<Self> {
        let dirs = ProjectDirs::from("", "", "torrx")?;
        let state = dirs
            .state_dir()
            .map_or_else(|| dirs.data_dir().to_path_buf(), Path::to_path_buf);
        Some(Self::new(dirs.config_dir().to_path_buf(), state))
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn session_dir(&self) -> PathBuf {
        self.state.join("session")
    }

    pub fn dht_file(&self) -> PathBuf {
        self.state.join("dht.json")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.state.join("logs")
    }
}

pub fn default_download_dir() -> PathBuf {
    UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_well_known_paths() {
        let dirs = AppDirs::new(PathBuf::from("/cfg"), PathBuf::from("/state"));
        assert_eq!(dirs.config_file(), Path::new("/cfg/config.toml"));
        assert_eq!(dirs.session_dir(), Path::new("/state/session"));
        assert_eq!(dirs.dht_file(), Path::new("/state/dht.json"));
        assert_eq!(dirs.log_dir(), Path::new("/state/logs"));
    }
}
