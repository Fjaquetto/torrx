# torrx

[![ci](https://github.com/Fjaquetto/torrx/actions/workflows/ci.yml/badge.svg)](https://github.com/Fjaquetto/torrx/actions/workflows/ci.yml)

Fast, secure and minimal BitTorrent client for the terminal.

`torrx` downloads and seeds torrents with a clean, keyboard-driven TUI: progress
bars, live download/upload speeds, ETA, peers, ratio and a speed history chart —
plus headless subcommands for scripts.

```
┌ torrx · balanced · dht on · port 51413 ────────────────────────────────────────────────────────┐
│↓ 8.00 MiB/s   ↑ 786.00 KiB/s                                 ↓ last 120s                        │
│session ↓ 13.78 GiB  ↑ 4.94 GiB                                  ▅  ▂   ▅  ▂   ▅  ▂   ▅  ▂       │
│peers 42 · uptime 01:30:00                                     ▁▇█ ▄█ ▁▇█ ▄█ ▁▇█ ▄█ ▁▇█ ▄█       │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
┌ torrents ───────────────────────────────────────────────────────────────────────────────────────┐
│  st     name                      size      progress                         done   down        │
│▌ down   debian-13.0-amd64-netinst 3.45 GiB  ██████████░░░░░░░░░░░░░░░░░░░░░░ 32.4%  3.05 MiB/s  │
│  seed   ubuntu-26.04-desktop-amd6 3.82 GiB  ████████████████████████████████ 100%   0 B/s       │
└─────────────────────────────────────────────────────────────────────────────────────────────────┘
 q quit · a add · space pause/resume · d remove · enter details · ? help
```

## Features

- Download **and** upload (seeding) with per-session and global views
- TUI with progress bars, ↓/↑ speeds, ETA, peers, ratio and a 120 s speed chart
- Torrent detail screen: per-file progress, file selection, info hash, folder
- Add torrents from magnet links, `.torrent` files or http(s) URLs
- Pause, resume and remove (keeping or deleting the data)
- Session persistence with fastresume: everything continues after a restart
- DHT, local peer discovery, uTP/TCP, UPnP (opt-in), upload/download rate limits
- Two security profiles: `balanced` (default) and `private`
- Headless subcommands: `add`, `list`, `info`, `pause`, `resume`, `remove`

## Install

Requires a Rust toolchain (stable). Build from source:

```sh
cargo install --path .          # or: cargo build --release
```

The engine is [`librqbit`](https://github.com/ikatson/rqbit) (pure Rust, Apache-2.0),
built here with rustls — no OpenSSL required.

## Usage

Run `torrx` with no arguments to open the TUI. The CLI is self-documenting:
`torrx --help`, `torrx help <command>` and `torrx <command> --help` (`-h` works
too) show every command, argument and flag.

### TUI keys

| Key | Action |
| --- | --- |
| `a` | add a magnet link, URL or `.torrent` path |
| `←` `→` `Home` `End` | move the cursor while typing a source |
| `Ctrl+W` / `Ctrl+U` | delete the previous word / clear the add field |
| `space` / `p` | pause, resume when paused |
| `r` | resume the selected torrent |
| `d` | remove the selected torrent |
| `enter` | open details (files, info hash, folder) |
| `↑` `↓` `PgUp` `PgDn` | move through the list |
| `?` | help overlay |
| `q` / `ctrl-c` | quit |

In the add dialog, paste with your terminal shortcut (`Ctrl+Shift+V`, right
click, middle click) — torrx enables bracketed paste, so paths with spaces and
long lines arrive intact. Sources can be absolute, relative, quoted, `~/...` or
`file:///...` URLs.

In the detail screen `space` includes/excludes the file under the cursor and
`esc` goes back.

### Commands

| Command | What it does | Example |
| --- | --- | --- |
| `add <SOURCE>...` | adds one or more torrents (magnet, URL or file) | `torrx add ./debian.torrent 'magnet:?xt=urn:btih:...'` |
| `list` | table of torrents: id, state, size, done, speeds, ratio | `torrx list` |
| `info <ID\|HASH>` | torrent details, including per-file progress | `torrx info 0` |
| `pause <ID\|HASH>` / `resume <ID\|HASH>` | stop or restart a torrent | `torrx resume 33315a` |
| `remove <ID\|HASH>` | forget a torrent; `--delete-files` also deletes the data | `torrx remove 0 --delete-files` |
| `config-path` | prints the config file location | `torrx config-path` |

`info`, `pause`, `resume` and `remove` accept the numeric id shown by
`torrx list` or any unique prefix (4+ characters) of the info hash. Add
`--paused` to `add` to queue a torrent without starting it.

### Global flags

Work both before and after the subcommand, e.g. `torrx --profile private list`
or `torrx list --profile private`:

| Flag | Meaning |
| --- | --- |
| `--profile <balanced\|private>` | security profile preset |
| `-o, --output <DIR>` | download directory |
| `--config <PATH>` | alternate config file |
| `-v` / `-q` | more/less logging (repeatable) |

Transfers advance while `torrx` is running; run the TUI inside `tmux` (or as a
service) when you want it to keep seeding.

## Configuration

The config file is `~/.config/torrx/config.toml` (`torrx config-path` prints the
exact location; `--config PATH` overrides it). Every key is optional and falls
back to the profile preset:

```toml
profile = "balanced"        # balanced | private
download_dir = "~/Downloads"
listen = true               # accept incoming connections (required to seed well)
listen_port = 0             # 0 = random port on start
dht = true                  # distributed hash table (peer discovery)
lsd = true                  # local peer discovery on the LAN
trackers = true             # announce to trackers
upnp = false                # do not open the router port automatically
seed = true                 # keep uploading after the download completes
peer_limit = 200
upload_limit_kib = 0        # KiB/s, 0 = unlimited
download_limit_kib = 0      # KiB/s, 0 = unlimited
log_level = "info"          # trace | debug | info | warn | error
```

Profiles:

| Profile | DHT | LSD | Trackers | UPnP | Seeding |
| --- | --- | --- | --- | --- | --- |
| `balanced` (default) | on | on | on | off | on |
| `private` | off | off | on | off | off |

`private` is tracker-only discovery for privacy-focused setups; with seeding off,
torrents are paused automatically when they finish. `--profile` and the other
flags override the config file. `RUST_LOG` takes precedence over `log_level`.

Paths: config in `~/.config/torrx`, session/fastresume state and logs in
`~/.local/state/torrx`.

## Security

- **Path traversal is rejected**: torrent names containing `..` or absolute
  components never escape the download directory (guarded by the engine and
  covered by tests).
- **No local API/server** by default — only the BitTorrent listener, which you
  can disable with `listen = false`.
- **UPnP is off by default**: no router ports are opened unless you ask for it.
- **Private mode** turns off DHT and LAN discovery and disables seeding.
- `#![forbid(unsafe_code)]` in this crate; no shell execution anywhere; no
  torrent data is ever interpreted as a command.
- Session, DHT and log state live in `~/.local/state/torrx`, which is created
  with `0700` permissions; `torrx` warns when it is started as root.
- Logs go to `~/.local/state/torrx/logs` instead of stdout, so nothing leaks
  into the TUI or into piped output.
- Supply chain: `cargo-deny` (advisories, licenses, sources) passes; CI runs
  `cargo clippy -D warnings`, `rustfmt` and the full test suite.
- Known limitations:
  - The engine does not implement protocol encryption (MSE/PE), so the wire
    protocol is unencrypted. Piece integrity is still checked (SHA-1, per the
    spec).
  - The listener runs in TCP-only mode; uTP is not enabled yet.
  - `RUSTSEC-2026-0293` (`ringbuf 0.4.8`, reachable only through uTP) is ignored
    in `deny.toml` with a documented reason: the newest `librqbit-utp` still
    pins `^0.4` and the affected code path is not exercised by a TCP-only
    listener. The advisory will be dropped as soon as upstream allows the fix.

## Architecture

```
src/
├── main.rs, lib.rs      # thin entrypoint + module wiring
├── cli.rs               # clap definitions (subcommands, flags)
├── config/              # TOML config, profile presets, validation
├── domain/              # pure types: views, statuses, selector resolution
├── format/              # pure formatting: bytes, speeds, ETA, bars
├── engine/              # TorrentEngine trait + librqbit adapter + fake engine
├── app/                 # UI state machine: events in, effects out (no I/O)
├── ui/                  # ratatui rendering (header, table, detail, modals)
├── runtime/             # event loop, engine polling, headless commands
├── paths.rs, logging.rs
examples/                # create_torrent, preview (dev tools)
xtask/                   # release helpers (version bump + changelog), tested
tests/                   # config loading, headless flow, offline e2e transfer
```

Design rules:

- Only `engine/librqbit_engine.rs` knows about `librqbit`; everything else talks to
  the `TorrentEngine` trait, which keeps the UI testable with a fake engine.
- `app` is a pure state machine: `handle_event` returns the effects to run, so
  the whole interaction model is unit-tested without a terminal.
- Comments are rare on purpose; naming carries the intent.

## Releases

Merging `develop` into `main` runs `.github/workflows/release.yml`: it validates
the tree, analyzes Conventional Commits since the last tag, bumps `Cargo.toml`,
updates `CHANGELOG.md`, creates an annotated `vX.Y.Z` tag and publishes the
GitHub release with Linux (gnu/musl, x86_64/aarch64), macOS (x86_64/arm64) and
Windows binaries plus `sha256` checksums. A back-merge keeps `develop` in sync.
The first release (`v0.1.0`) is tagged manually to bootstrap the sequence.

The bot pushes the release commit to `main`/`develop`, so GitHub Actions needs a
ruleset bypass on those branches (or a `RELEASE_TOKEN` secret with the bypass).
The release body is generated from Conventional Commits by `cargo xtask`.

## Roadmap

- Enable uTP once `librqbit-utp` allows `ringbuf >= 0.5.2`
- Blocklist/allowlist support (`blocklist_url`), interface binding, fixed ports
- Completion notifications (desktop notification / terminal bell)
- Headless daemon mode so transfers keep running without a terminal
- Per-torrent rate limits and queueing

## Tests

```sh
cargo test --workspace --all-targets   # unit + integration + xtask (offline)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Covered by tests: formatting edge cases (0 B, NaN, huge ETA), config validation
and precedence (including real file loading), profile presets, source parsing
(`~`, quotes, `file://`), selector resolution by id/hash prefix, torrent and file
state mapping, the app state machine (selections, modals, paste editing,
auto-pause, status expiry), CLI parsing, key mapping, rendered TUI frames and
cursor placement (`TestBackend`), the release version/changelog logic (`xtask`)
and a real end-to-end transfer: a seeder, a leecher, and a second leecher that
downloads exclusively from the first one.
