use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    restrict_to_owner(path)
}

pub fn running_as_root() -> bool {
    first_uid_is_root(&read_uid_line())
}

pub fn expand_tilde(input: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(input);
    };
    if input == "~" {
        return home.to_path_buf();
    }
    input
        .strip_prefix("~/")
        .map_or_else(|| PathBuf::from(input), |rest| home.join(rest))
}

fn read_uid_line() -> String {
    fs::read_to_string("/proc/self/status").unwrap_or_default()
}

fn first_uid_is_root(status: &str) -> bool {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|uids| uids.split_whitespace().next())
        .is_some_and(|uid| uid == "0")
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_root_uid() {
        assert!(first_uid_is_root("Name:\ttorrx\nUid:\t0\t0\t0\t0\n"));
        assert!(first_uid_is_root("Uid: 0 1000 1000 1000"));
    }

    #[test]
    fn expands_tilde_against_home() {
        let home = Path::new("/home/user");
        assert_eq!(
            expand_tilde("~/dl", Some(home)),
            PathBuf::from("/home/user/dl")
        );
        assert_eq!(expand_tilde("~", Some(home)), PathBuf::from("/home/user"));
        assert_eq!(expand_tilde("/abs", Some(home)), PathBuf::from("/abs"));
        assert_eq!(
            expand_tilde("~other/x", Some(home)),
            PathBuf::from("~other/x")
        );
        assert_eq!(expand_tilde("~/dl", None), PathBuf::from("~/dl"));
    }

    #[test]
    fn ignores_non_root_and_garbage() {
        assert!(!first_uid_is_root("Uid:\t1000\t1000\t1000\t1000\n"));
        assert!(!first_uid_is_root("Uid:\t10000\t1000\t1000\t1000\n"));
        assert!(!first_uid_is_root("no uid here"));
        assert!(!first_uid_is_root(""));
    }

    #[cfg(unix)]
    #[test]
    fn creates_owner_only_directory() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::TempDir::new().expect("temp dir");
        let path = root.path().join("nested/state");
        create_private_dir(&path).expect("creates directory");

        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
