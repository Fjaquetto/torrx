//! Release helpers: analyze Conventional Commits, bump the version and build
//! the changelog used by the release workflow.

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, value};

pub const BASE_TAG_FALLBACK: &str = "v0.0.0";

const PATCH_TYPES: [&str; 7] = ["fix", "chore", "docs", "style", "refactor", "perf", "test"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl Bump {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub base: String,
    pub version: String,
    pub bump: Bump,
}

pub fn analyze(base: &str, commits: &[Commit]) -> Option<Plan> {
    let bump = commits.iter().filter_map(commit_bump).max()?;
    let (major, minor, patch) = parse_version(base).unwrap_or((0, 0, 0));
    let (major, minor, patch) = match bump {
        Bump::Major => (major + 1, 0, 0),
        Bump::Minor => (major, minor + 1, 0),
        Bump::Patch => (major, minor, patch + 1),
    };
    Some(Plan {
        base: base.to_owned(),
        version: format!("{major}.{minor}.{patch}"),
        bump,
    })
}

pub fn commit_bump(commit: &Commit) -> Option<Bump> {
    if is_ignored(&commit.subject) {
        return None;
    }
    if commit.body.contains("BREAKING CHANGE:") || subject_is_breaking(&commit.subject) {
        return Some(Bump::Major);
    }
    match commit_kind(&commit.subject)? {
        "feat" => Some(Bump::Minor),
        kind if PATCH_TYPES.contains(&kind) => Some(Bump::Patch),
        _ => None,
    }
}

pub fn changelog_section(version: &str, commits: &[Commit]) -> String {
    let mut breaking = Vec::new();
    let mut features = Vec::new();
    let mut fixes = Vec::new();
    let mut other = Vec::new();

    for commit in commits {
        if is_ignored(&commit.subject) {
            continue;
        }
        let line = format!("- {} (`{}`)", commit.subject, short_hash(&commit.hash));
        if commit.body.contains("BREAKING CHANGE:") || subject_is_breaking(&commit.subject) {
            breaking.push(line);
            continue;
        }
        match commit_kind(&commit.subject) {
            Some("feat") => features.push(line),
            Some("fix") => fixes.push(line),
            Some(kind) if PATCH_TYPES.contains(&kind) => other.push(line),
            _ => {}
        }
    }

    let mut section = format!("## {version}\n\n");
    push_group(&mut section, "Breaking changes", &breaking);
    push_group(&mut section, "Features", &features);
    push_group(&mut section, "Bug fixes", &fixes);
    push_group(&mut section, "Other changes", &other);
    section
}

pub fn insert_changelog(existing: &str, section: &str) -> String {
    let section = section.trim_end();
    if let Some(index) = existing.find("\n## ") {
        let head = &existing[..=index];
        let tail = &existing[index + 1..];
        return format!("{head}{section}\n\n{tail}");
    }
    if existing.trim().is_empty() {
        return format!("# Changelog\n\n{section}\n");
    }
    format!("{}\n\n{section}\n", existing.trim_end())
}

pub fn bump_manifest(contents: &str, version: &str) -> Result<String> {
    let mut document = contents
        .parse::<DocumentMut>()
        .context("invalid Cargo.toml")?;
    document["package"]["version"] = value(version);
    Ok(document.to_string())
}

pub fn latest_tag() -> String {
    Command::new("git")
        .args(["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .unwrap_or_else(|| BASE_TAG_FALLBACK.to_owned())
}

pub fn commits_since(base: &str) -> Result<Vec<Commit>> {
    let range = if base == BASE_TAG_FALLBACK {
        "HEAD".to_owned()
    } else {
        format!("{base}..HEAD")
    };
    let output = Command::new("git")
        .args(["log", "--reverse", "--format=%H%x1f%s%x1f%b%x1e", &range])
        .output()
        .context("failed to run git log")?;
    if !output.status.success() {
        anyhow::bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();
    for record in text.split('\x1e') {
        let record = record.trim_matches('\n');
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, '\x1f');
        let (Some(hash), Some(subject), Some(body)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        commits.push(Commit {
            hash: hash.trim().to_owned(),
            subject: subject.trim().to_owned(),
            body: body.trim().to_owned(),
        });
    }
    Ok(commits)
}

pub fn write_manifest(path: &Path, version: &str) -> Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let updated = bump_manifest(&contents, version)?;
    fs::write(path, updated).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn push_group(section: &mut String, title: &str, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    section.push_str("### ");
    section.push_str(title);
    section.push('\n');
    for line in lines {
        section.push_str(line);
        section.push('\n');
    }
    section.push('\n');
}

fn short_hash(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim_start_matches('v').split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

fn is_ignored(subject: &str) -> bool {
    subject.starts_with("Merge ") || subject.starts_with("chore(release):")
}

fn subject_is_breaking(subject: &str) -> bool {
    subject
        .split_once(':')
        .is_some_and(|(head, _)| head.trim_end().ends_with('!'))
}

fn commit_kind(subject: &str) -> Option<&str> {
    let (head, _) = subject.split_once(':')?;
    let head = head.trim().trim_end_matches('!');
    head.split('(').next().map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(subject: &str, body: &str) -> Commit {
        Commit {
            hash: "0123456789abcdef".to_owned(),
            subject: subject.to_owned(),
            body: body.to_owned(),
        }
    }

    #[test]
    fn minors_and_patches_follow_conventional_commits() {
        let plan = analyze("v1.2.3", &[commit("feat: add search", "")]).expect("minor");
        assert_eq!(plan.version, "1.3.0");
        assert_eq!(plan.bump, Bump::Minor);

        let plan = analyze("v1.2.3", &[commit("fix(ui): align table", "")]).expect("patch");
        assert_eq!(plan.version, "1.2.4");
        assert_eq!(plan.bump, Bump::Patch);

        let plan = analyze("v0.0.0", &[commit("docs: update readme", "")]).expect("patch");
        assert_eq!(plan.version, "0.0.1");
    }

    #[test]
    fn breaking_changes_win_over_features() {
        let commits = [
            commit("feat: something", ""),
            commit("refactor!: drop legacy flags", ""),
        ];
        let plan = analyze("v1.2.3", &commits).expect("major");
        assert_eq!(plan.version, "2.0.0");
        assert_eq!(plan.bump, Bump::Major);

        let body = "fix: patch\n\nBREAKING CHANGE: config format changed";
        let plan = analyze("v1.2.3", &[commit("fix: patch", body)]).expect("major");
        assert_eq!(plan.version, "2.0.0");
    }

    #[test]
    fn ignores_merges_and_release_commits() {
        let commits = [
            commit("Merge pull request #1 from x/develop", ""),
            commit("chore(release): v1.2.4 [skip ci]", ""),
            commit("ci: bump actions", ""),
            commit("build: update lockfile", ""),
        ];
        assert!(analyze("v1.2.3", &commits).is_none());
    }

    #[test]
    fn groups_the_changelog_and_skips_noise() {
        let commits = [
            commit("feat: add paste", ""),
            commit("fix: keep cursor", ""),
            commit("docs: document keys", ""),
            commit("chore(release): v1.3.0 [skip ci]", ""),
            commit("Merge branch develop", ""),
        ];
        let section = changelog_section("v1.3.0", &commits);
        assert!(section.starts_with("## v1.3.0\n\n"));
        assert!(section.contains("### Features"));
        assert!(section.contains("- feat: add paste (`0123456`)"));
        assert!(section.contains("### Bug fixes"));
        assert!(section.contains("### Other changes"));
        assert!(!section.contains("chore(release)"));
        assert!(!section.contains("Merge branch"));
    }

    #[test]
    fn inserts_new_section_below_the_intro_and_before_previous_releases() {
        let existing = "# Changelog\n\nAll notable changes.\n\n## v1.0.0\n\n- old\n";
        let updated = insert_changelog(existing, "## v1.1.0\n\n- new\n");
        assert!(updated.starts_with("# Changelog\n\nAll notable changes."));
        assert!(updated.find("## v1.1.0") < updated.find("## v1.0.0"));
        assert!(updated.contains("- new"));
        assert!(updated.contains("- old"));
    }

    #[test]
    fn creates_changelog_when_missing() {
        let updated = insert_changelog("", "## v0.1.0\n\n- first\n");
        assert_eq!(updated, "# Changelog\n\n## v0.1.0\n\n- first\n");
    }

    #[test]
    fn bumps_manifest_preserving_comments_and_fields() {
        let contents = r#"# top comment
[package]
name = "torrx"
version = "0.1.0" # current
edition = "2024"

[dependencies]
anyhow = "1.0"
"#;
        let updated = bump_manifest(contents, "0.2.0").expect("valid manifest");
        assert!(updated.contains("version = \"0.2.0\""));
        assert!(updated.contains("# top comment"));
        assert!(updated.contains("name = \"torrx\""));
        assert!(updated.contains("edition = \"2024\""));
    }

    #[test]
    fn parses_versions_with_prefixes_and_suffixes() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-rc.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("not-a-version"), None);
    }
}
