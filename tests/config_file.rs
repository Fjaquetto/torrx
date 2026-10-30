use std::path::Path;

use tempfile::TempDir;
use torrx::{
    config::{self, CliOverrides, Profile},
    paths::AppDirs,
};

fn app_dirs(root: &Path) -> AppDirs {
    AppDirs::new(root.join("config"), root.join("state"))
}

#[test]
fn loads_config_file_and_applies_cli_precedence() {
    let root = TempDir::new().expect("temp dir");
    let path = root.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
profile = "private"
download_dir = "/tmp/torrx-downloads"
peer_limit = 75
upload_limit_kib = 512
seed = true
"#,
    )
    .expect("writes config");

    let cli = CliOverrides {
        profile: Some(Profile::Balanced),
        ..CliOverrides::default()
    };
    let config = config::load(Some(&path), &app_dirs(root.path()), &cli).expect("loads config");

    assert_eq!(config.profile, Profile::Balanced);
    assert!(config.dht);
    assert_eq!(config.peer_limit, 75);
    assert_eq!(config.upload_limit_kib, 512);
    assert!(config.seed);
    assert_eq!(config.download_dir, Path::new("/tmp/torrx-downloads"));
    assert_eq!(config.config_path.as_deref(), Some(path.as_path()));
}

#[test]
fn uses_defaults_when_no_config_file_exists() {
    let root = TempDir::new().expect("temp dir");
    let config = config::load(None, &app_dirs(root.path()), &CliOverrides::default())
        .expect("defaults are valid");

    assert_eq!(config.profile, Profile::Balanced);
    assert!(config.dht);
    assert!(!config.upnp);
    assert_eq!(config.config_path, None);
}
