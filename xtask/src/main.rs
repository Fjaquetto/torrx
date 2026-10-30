use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::Result;
use clap::{Parser, Subcommand};
use xtask::{
    analyze, changelog_section, commits_since, insert_changelog, latest_tag, write_manifest,
};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Release helpers for torrx")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the next release plan as JSON (exit code 2 when nothing to release).
    NextVersion,
    /// Build the changelog for a version, optionally updating CHANGELOG.md.
    Changelog {
        #[arg(long)]
        from: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        update: Option<PathBuf>,
    },
    /// Update the version in Cargo.toml.
    Bump { version: String },
}

fn main() -> Result<ExitCode> {
    match Cli::parse().command {
        Command::NextVersion => {
            let base = latest_tag();
            let commits = commits_since(&base)?;
            let Some(plan) = analyze(&base, &commits) else {
                return Ok(ExitCode::from(2));
            };
            println!(
                "{}",
                serde_json::json!({
                    "base": plan.base,
                    "version": plan.version,
                    "bump": plan.bump.as_str(),
                })
            );
        }
        Command::Changelog {
            from,
            version,
            update,
        } => {
            let commits = commits_since(&from)?;
            let section = changelog_section(&version, &commits);
            if let Some(path) = update {
                let existing = fs::read_to_string(&path).unwrap_or_default();
                fs::write(&path, insert_changelog(&existing, &section))?;
            }
            print!("{section}");
        }
        Command::Bump { version } => {
            write_manifest(Path::new("Cargo.toml"), &version)?;
            println!("bumped Cargo.toml to {version}");
        }
    }
    Ok(ExitCode::SUCCESS)
}
