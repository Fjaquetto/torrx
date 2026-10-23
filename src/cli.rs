use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

use crate::config::{CliOverrides, Profile};

#[derive(Debug, Parser)]
#[command(
    name = "torrx",
    version,
    about = "Fast, secure and minimal BitTorrent client for the terminal"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long, global = true, value_name = "PATH", help = "Config file to use")]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, value_enum, help = "Security profile preset")]
    pub profile: Option<Profile>,

    #[arg(
        long,
        short = 'o',
        global = true,
        value_name = "DIR",
        help = "Download directory"
    )]
    pub output: Option<PathBuf>,

    #[arg(long, short = 'v', global = true, action = ArgAction::Count, help = "Increase log verbosity (repeatable)")]
    pub verbose: u8,

    #[arg(
        long,
        short = 'q',
        global = true,
        help = "Only log warnings and errors"
    )]
    pub quiet: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Add torrents by magnet link, .torrent file or URL")]
    Add {
        #[arg(
            required = true,
            value_name = "SOURCE",
            help = "Magnet link, http(s) .torrent URL or path to a .torrent file"
        )]
        sources: Vec<String>,
        #[arg(long, help = "Add without starting the download")]
        paused: bool,
    },
    #[command(about = "List torrents")]
    List,
    #[command(about = "Show torrent details")]
    Info {
        #[arg(value_name = "ID|HASH", help = SELECTOR_HELP)]
        selector: String,
    },
    #[command(about = "Pause a torrent")]
    Pause {
        #[arg(value_name = "ID|HASH", help = SELECTOR_HELP)]
        selector: String,
    },
    #[command(about = "Resume a torrent")]
    Resume {
        #[arg(value_name = "ID|HASH", help = SELECTOR_HELP)]
        selector: String,
    },
    #[command(about = "Remove a torrent")]
    Remove {
        #[arg(value_name = "ID|HASH", help = SELECTOR_HELP)]
        selector: String,
        #[arg(long, help = "Also delete the downloaded data")]
        delete_files: bool,
    },
    #[command(about = "Print the path of the config file")]
    ConfigPath,
}

const SELECTOR_HELP: &str =
    "Torrent id from `torrx list`, or a unique prefix of its info hash (4+ characters)";

impl Cli {
    pub fn overrides(&self) -> CliOverrides {
        CliOverrides {
            profile: self.profile,
            download_dir: self.output.clone(),
            verbose: self.verbose,
            quiet: self.quiet,
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("valid arguments")
    }

    #[test]
    fn parses_no_arguments_as_tui_mode() {
        let cli = parse(&["torrx"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn parses_add_with_multiple_sources() {
        let cli = parse(&["torrx", "add", "magnet:?xt=urn:btih:aa", "./file.torrent"]);
        let Some(Command::Add { sources, paused }) = cli.command else {
            panic!("expected add command");
        };
        assert_eq!(sources.len(), 2);
        assert!(!paused);
    }

    #[test]
    fn parses_global_flags_before_subcommand() {
        let cli = parse(&["torrx", "--profile", "private", "-v", "list"]);
        assert_eq!(cli.profile, Some(Profile::Private));
        assert_eq!(cli.verbose, 1);
        assert!(matches!(cli.command, Some(Command::List)));
    }

    #[test]
    fn parses_global_flags_after_subcommand() {
        let cli = parse(&[
            "torrx",
            "list",
            "--profile",
            "private",
            "-v",
            "-o",
            "/tmp/dl",
        ]);
        assert_eq!(cli.profile, Some(Profile::Private));
        assert_eq!(cli.verbose, 1);
        assert_eq!(cli.output.as_deref(), Some(std::path::Path::new("/tmp/dl")));
    }

    #[test]
    fn parses_remove_with_delete_files() {
        let cli = parse(&["torrx", "remove", "3", "--delete-files"]);
        let Some(Command::Remove {
            selector,
            delete_files,
        }) = cli.command
        else {
            panic!("expected remove command");
        };
        assert_eq!(selector, "3");
        assert!(delete_files);
    }

    #[test]
    fn rejects_unknown_profile() {
        assert!(Cli::try_parse_from(["torrx", "--profile", "rogue"]).is_err());
    }

    #[test]
    fn builds_cli_overrides() {
        let cli = parse(&["torrx", "--profile", "private", "-o", "/dl", "-vv"]);
        let overrides = cli.overrides();
        assert_eq!(overrides.profile, Some(Profile::Private));
        assert_eq!(overrides.verbose, 2);
        assert_eq!(
            overrides.download_dir.as_deref(),
            Some(std::path::Path::new("/dl"))
        );
    }
}
