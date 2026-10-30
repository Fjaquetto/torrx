mod commands;

pub use commands::run_command;

use std::{io::stdout, sync::Arc, time::Duration};

use anyhow::Context;
use crossterm::{
    event::{
        DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
        KeyEventKind, KeyModifiers,
    },
    execute,
};
use futures_util::StreamExt;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::{
    app::{ActionOk, App, AppEvent, Effect, Key},
    config::Config,
    engine::{AddOptions, EngineError, LibrqbitEngine, TorrentEngine},
    paths::{self, AppDirs},
    platform, ui,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const TICK_INTERVAL: Duration = Duration::from_millis(250);
const CHANNEL_CAPACITY: usize = 64;

pub async fn run_tui(config: &Config, dirs: &AppDirs) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.download_dir).with_context(|| {
        format!(
            "cannot create download directory {}",
            config.download_dir.display()
        )
    })?;
    platform::create_private_dir(&dirs.session_dir()).context("cannot create session directory")?;

    let engine: Arc<dyn TorrentEngine> =
        Arc::new(LibrqbitEngine::open(config, &dirs.session_dir(), &dirs.dht_file()).await?);

    let (tx, mut rx) = mpsc::channel::<AppEvent>(CHANNEL_CAPACITY);
    spawn_poller(engine.clone(), tx.clone());

    let mut app = App::new(config.profile.as_str(), config.seed, paths::home_dir());
    let mut terminal = ratatui::try_init().context(
        "cannot start the terminal UI; run a subcommand such as `torrx list` when there is no TTY",
    )?;
    let paste_guard = BracketedPasteGuard::enable();
    let result = event_loop(&mut terminal, &engine, tx, &mut rx, &mut app).await;
    drop(paste_guard);
    ratatui::restore();
    engine.shutdown().await;

    result
}

struct TerminationSignal {
    inner: Option<tokio::signal::unix::Signal>,
}

impl TerminationSignal {
    fn install() -> Self {
        Self {
            inner: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok(),
        }
    }

    async fn wait(&mut self) {
        if let Some(signal) = self.inner.as_mut() {
            signal.recv().await;
            return;
        }
        std::future::pending::<()>().await;
    }
}

struct BracketedPasteGuard;

impl BracketedPasteGuard {
    fn enable() -> Self {
        let _ = execute!(stdout(), EnableBracketedPaste);
        Self
    }
}

impl Drop for BracketedPasteGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableBracketedPaste);
    }
}

async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    engine: &Arc<dyn TorrentEngine>,
    tx: Sender<AppEvent>,
    rx: &mut Receiver<AppEvent>,
    app: &mut App,
) -> anyhow::Result<()> {
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK_INTERVAL);
    let mut termination = TerminationSignal::install();

    loop {
        let event = tokio::select! {
            maybe = events.next() => match maybe {
                Some(Ok(event)) => match terminal_event_to_app(&event) {
                    Some(event) => event,
                    None => continue,
                },
                _ => break,
            },
            Some(event) = rx.recv() => event,
            _ = ticker.tick() => AppEvent::Tick,
            () = termination.wait() => AppEvent::Keys(vec![Key::CtrlC]),
        };

        let mut quit = false;
        for effect in app.handle_event(event) {
            if effect == Effect::Quit {
                quit = true;
            } else {
                execute_effect(effect, engine, &tx).await;
            }
        }

        terminal.draw(|frame| ui::render(frame, app))?;
        if quit {
            break;
        }
    }
    Ok(())
}

fn spawn_poller(engine: Arc<dyn TorrentEngine>, tx: Sender<AppEvent>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        loop {
            interval.tick().await;
            let snapshot = engine.snapshot().await;
            if tx.send(AppEvent::Snapshot(snapshot)).await.is_err() {
                break;
            }
        }
    });
}

async fn execute_effect(effect: Effect, engine: &Arc<dyn TorrentEngine>, tx: &Sender<AppEvent>) {
    match effect {
        Effect::Add { source } => {
            let failed_source = source.clone();
            let engine = engine.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let event = match engine.add(source, AddOptions::default()).await {
                    Ok(outcome) => AppEvent::Action(Ok(ActionOk::Added(outcome))),
                    Err(error) => AppEvent::AddFailed {
                        source: failed_source,
                        error: error.to_string(),
                    },
                };
                let _ = tx.send(event).await;
            });
        }
        Effect::Pause(id) => {
            report(engine.pause(id).await.map(|()| ActionOk::Paused(id)), tx).await;
        }
        Effect::Resume(id) => {
            report(engine.resume(id).await.map(|()| ActionOk::Resumed(id)), tx).await;
        }
        Effect::Remove { id, delete_files } => {
            report(
                engine
                    .remove(id, delete_files)
                    .await
                    .map(|()| ActionOk::Removed {
                        identifier: format!("#{id}"),
                        delete_files,
                    }),
                tx,
            )
            .await;
        }
        Effect::LoadDetails(id) => {
            let details = engine.details(id).await;
            let _ = tx.send(AppEvent::Details(details)).await;
        }
        Effect::SetIncludedFiles { id, included } => {
            report(
                engine
                    .set_included_files(id, included)
                    .await
                    .map(|()| ActionOk::FilesUpdated(id)),
                tx,
            )
            .await;
        }
        Effect::Quit => {}
    }
}

async fn report(result: Result<ActionOk, EngineError>, tx: &Sender<AppEvent>) {
    let _ = tx.send(AppEvent::Action(result)).await;
}

fn terminal_event_to_app(event: &Event) -> Option<AppEvent> {
    match event {
        Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
            Some(AppEvent::Keys(vec![map_key(*key)]))
        }
        Event::Paste(text) => Some(AppEvent::Paste(text.clone())),
        Event::Resize(_, _) => Some(AppEvent::Tick),
        _ => None,
    }
}

fn map_key(key: KeyEvent) -> Key {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    match key.code {
        KeyCode::Char('c') if control => Key::CtrlC,
        KeyCode::Char('u') if control => Key::CtrlU,
        KeyCode::Char('w') if control => Key::CtrlW,
        KeyCode::Char('a') if control => Key::Home,
        KeyCode::Char('e') if control => Key::End,
        KeyCode::Char(c) if !control && !alt => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        _ => Key::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::AddSource;
    use crate::engine::fake::FakeEngine;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn engine_dyn() -> (Arc<dyn TorrentEngine>, Arc<FakeEngine>) {
        let fake = Arc::new(FakeEngine::default());
        let engine: Arc<dyn TorrentEngine> = fake.clone();
        (engine, fake)
    }

    #[tokio::test]
    async fn pause_effect_hits_the_engine_and_reports_back() {
        let (engine, fake) = engine_dyn();
        let (tx, mut rx) = mpsc::channel(4);

        execute_effect(Effect::Pause(crate::domain::TorrentId(3)), &engine, &tx).await;
        let event = rx.recv().await.expect("action event");

        assert!(matches!(
            event,
            AppEvent::Action(Ok(ActionOk::Paused(crate::domain::TorrentId(3))))
        ));
        assert_eq!(fake.calls(), vec!["pause 3"]);
    }

    #[tokio::test]
    async fn remove_effect_reports_identifier_and_delete_flag() {
        let (engine, fake) = engine_dyn();
        let (tx, mut rx) = mpsc::channel(4);

        execute_effect(
            Effect::Remove {
                id: crate::domain::TorrentId(7),
                delete_files: true,
            },
            &engine,
            &tx,
        )
        .await;

        assert!(matches!(
            rx.recv().await.expect("action event"),
            AppEvent::Action(Ok(ActionOk::Removed {
                delete_files: true,
                ..
            }))
        ));
        assert_eq!(fake.calls(), vec!["remove 7 delete_files=true"]);
    }

    #[tokio::test]
    async fn add_effect_runs_in_background_and_reports_failure() {
        let (engine, _fake) = engine_dyn();
        let (tx, mut rx) = mpsc::channel(4);

        execute_effect(
            Effect::Add {
                source: AddSource::parse("magnet:?xt=urn:btih:aa", None).expect("valid magnet"),
            },
            &engine,
            &tx,
        )
        .await;

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("add reports back")
            .expect("action event");
        assert!(matches!(event, AppEvent::AddFailed { .. }));
    }

    #[test]
    fn maps_keys_with_control_modifiers() {
        assert_eq!(
            map_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Key::CtrlC
        );
        assert_eq!(
            map_key(key(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Key::CtrlU
        );
        assert_eq!(
            map_key(key(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            Key::CtrlW
        );
        assert_eq!(
            map_key(key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            Key::Home
        );
        assert_eq!(
            map_key(key(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            Key::End
        );
    }

    #[test]
    fn control_letters_never_become_text() {
        assert_eq!(
            map_key(key(KeyCode::Char('v'), KeyModifiers::CONTROL)),
            Key::Other
        );
        assert_eq!(
            map_key(key(
                KeyCode::Char('v'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Key::Other
        );
        assert_eq!(
            map_key(key(KeyCode::Char('x'), KeyModifiers::ALT)),
            Key::Other
        );
    }

    #[test]
    fn maps_plain_keys() {
        assert_eq!(
            map_key(key(KeyCode::Char('a'), KeyModifiers::NONE)),
            Key::Char('a')
        );
        assert_eq!(map_key(key(KeyCode::Enter, KeyModifiers::NONE)), Key::Enter);
        assert_eq!(map_key(key(KeyCode::Esc, KeyModifiers::NONE)), Key::Esc);
        assert_eq!(map_key(key(KeyCode::Up, KeyModifiers::NONE)), Key::Up);
        assert_eq!(map_key(key(KeyCode::Left, KeyModifiers::NONE)), Key::Left);
        assert_eq!(map_key(key(KeyCode::Right, KeyModifiers::NONE)), Key::Right);
        assert_eq!(
            map_key(key(KeyCode::Delete, KeyModifiers::NONE)),
            Key::Delete
        );
        assert_eq!(
            map_key(key(KeyCode::PageDown, KeyModifiers::NONE)),
            Key::PageDown
        );
        assert_eq!(map_key(key(KeyCode::F(5), KeyModifiers::NONE)), Key::Other);
    }

    #[test]
    fn maps_bracketed_paste_events() {
        assert!(matches!(
            terminal_event_to_app(&Event::Paste("/tmp/a.torrent".to_owned())),
            Some(AppEvent::Paste(text)) if text == "/tmp/a.torrent"
        ));
    }

    #[test]
    fn ignores_key_releases_and_mouse_events() {
        let release = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let mut event = Event::Key(release);
        if let Event::Key(key) = &mut event {
            key.kind = KeyEventKind::Release;
        }
        assert!(terminal_event_to_app(&event).is_none());
        assert!(
            terminal_event_to_app(&Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Moved,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }))
            .is_none()
        );
    }
}
