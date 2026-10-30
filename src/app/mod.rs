use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::domain::{
    GlobalView, Snapshot, SpeedHistory, TorrentDetails, TorrentId, TorrentStatus, TorrentView,
};
use crate::engine::{AddOutcome, AddSource, EngineError};

const STATUS_TTL: Duration = Duration::from_secs(5);
const HISTORY_CAPACITY: usize = 120;
const MAX_INPUT_LEN: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    CtrlC,
    CtrlU,
    CtrlW,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Detail,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputState {
    pub value: String,
    pub cursor: usize,
    pub error: Option<String>,
}

impl InputState {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self {
            value,
            cursor,
            error: None,
        }
    }

    pub fn char_count(&self) -> usize {
        self.value.chars().count()
    }

    pub fn insert_char(&mut self, c: char) {
        let mut buffer = [0; 4];
        self.insert_str(c.encode_utf8(&mut buffer));
    }

    pub fn insert_str(&mut self, text: &str) {
        let index = self.byte_index(self.cursor);
        self.value.insert_str(index, text);
        self.cursor += text.chars().count();
        self.error = None;
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let index = self.byte_index(self.cursor - 1);
        self.value.remove(index);
        self.cursor -= 1;
        self.error = None;
        true
    }

    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.char_count() {
            return false;
        }
        let index = self.byte_index(self.cursor);
        self.value.remove(index);
        self.error = None;
        true
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.char_count();
    }

    pub fn delete_word_back(&mut self) {
        while self.cursor > 0 && self.char_before(self.cursor).is_whitespace() {
            self.backspace();
        }
        while self.cursor > 0 && !self.char_before(self.cursor).is_whitespace() {
            self.backspace();
        }
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.error = None;
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    fn char_before(&self, char_index: usize) -> char {
        self.value.chars().nth(char_index - 1).unwrap_or(' ')
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_index)
            .map_or(self.value.len(), |(index, _)| index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmState {
    pub id: TorrentId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    Help,
    Input(InputState),
    Confirm(ConfirmState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub level: StatusLevel,
    pub created: Instant,
}

impl StatusMessage {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: StatusLevel::Info,
            created: Instant::now(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: StatusLevel::Error,
            created: Instant::now(),
        }
    }

    pub fn is_expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.created) > STATUS_TTL
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Add { source: AddSource },
    Pause(TorrentId),
    Resume(TorrentId),
    Remove { id: TorrentId, delete_files: bool },
    LoadDetails(TorrentId),
    SetIncludedFiles { id: TorrentId, included: Vec<usize> },
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOk {
    Added(AddOutcome),
    Paused(TorrentId),
    Resumed(TorrentId),
    Removed {
        identifier: String,
        delete_files: bool,
    },
    FilesUpdated(TorrentId),
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Snapshot(Snapshot),
    Details(Result<TorrentDetails, EngineError>),
    Action(Result<ActionOk, EngineError>),
    AddFailed { source: AddSource, error: String },
    Paste(String),
    Keys(Vec<Key>),
    Tick,
}

pub struct App {
    pub torrents: Vec<TorrentView>,
    pub global: GlobalView,
    pub selected_index: usize,
    pub screen: Screen,
    pub modal: Option<Modal>,
    pub status: Option<StatusMessage>,
    pub history: SpeedHistory,
    pub details: Option<TorrentDetails>,
    pub file_cursor: usize,
    pub profile: String,
    seed_after_complete: bool,
    pending_select: Option<TorrentId>,
    auto_paused: Vec<TorrentId>,
    home: Option<PathBuf>,
}

impl App {
    pub fn new(profile: &str, seed_after_complete: bool, home: Option<PathBuf>) -> Self {
        Self {
            torrents: Vec::new(),
            global: GlobalView::default(),
            selected_index: 0,
            screen: Screen::List,
            modal: None,
            status: None,
            history: SpeedHistory::new(HISTORY_CAPACITY),
            details: None,
            file_cursor: 0,
            profile: profile.to_owned(),
            seed_after_complete,
            pending_select: None,
            auto_paused: Vec::new(),
            home,
        }
    }

    pub fn handle_event(&mut self, event: AppEvent) -> Vec<Effect> {
        match event {
            AppEvent::Snapshot(snapshot) => self.on_snapshot(snapshot),
            AppEvent::Details(result) => self.on_details(result),
            AppEvent::Action(result) => self.on_action(result),
            AppEvent::AddFailed { source, error } => self.on_add_failed(&source, error),
            AppEvent::Paste(text) => self.on_paste(&text),
            AppEvent::Keys(keys) => keys.into_iter().flat_map(|key| self.on_key(key)).collect(),
            AppEvent::Tick => self.on_tick(),
        }
    }

    pub fn selected_torrent(&self) -> Option<&TorrentView> {
        self.torrents.get(self.selected_index)
    }

    pub fn selected_id(&self) -> Option<TorrentId> {
        self.selected_torrent().map(|torrent| torrent.id)
    }

    pub fn status_is_visible(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|status| !status.is_expired(Instant::now()))
    }

    fn on_snapshot(&mut self, snapshot: Snapshot) -> Vec<Effect> {
        let previous = self.selected_id();
        let previous_index = self.selected_index;

        self.torrents = snapshot.torrents;
        self.global = snapshot.global;
        self.history
            .push(self.global.download_speed, self.global.upload_speed);
        self.reselect(previous, previous_index);
        self.sync_details_view();
        self.auto_pause_effects()
    }

    fn reselect(&mut self, previous: Option<TorrentId>, previous_index: usize) {
        if self.torrents.is_empty() {
            self.selected_index = 0;
            return;
        }
        let target = self.pending_select.take().or(previous);
        self.selected_index = target
            .and_then(|id| self.torrents.iter().position(|torrent| torrent.id == id))
            .unwrap_or_else(|| previous_index.min(self.torrents.len() - 1));
    }

    fn sync_details_view(&mut self) {
        let Some(id) = self.details.as_ref().map(|details| details.view.id) else {
            return;
        };
        let view = self
            .torrents
            .iter()
            .find(|torrent| torrent.id == id)
            .cloned();
        if let Some(view) = view {
            if let Some(details) = self.details.as_mut() {
                details.view = view;
            }
        } else {
            self.details = None;
            self.screen = Screen::List;
        }
    }

    fn auto_pause_effects(&mut self) -> Vec<Effect> {
        if self.seed_after_complete {
            return Vec::new();
        }
        let mut effects = Vec::new();
        for torrent in &self.torrents {
            let should_pause = torrent.status == TorrentStatus::Seeding;
            if should_pause && !self.auto_paused.contains(&torrent.id) {
                self.auto_paused.push(torrent.id);
                effects.push(Effect::Pause(torrent.id));
            }
        }
        effects
    }

    fn on_details(&mut self, result: Result<TorrentDetails, EngineError>) -> Vec<Effect> {
        match result {
            Ok(details) => {
                self.details = Some(details);
                self.file_cursor = self.file_cursor.min(self.file_count().saturating_sub(1));
                Vec::new()
            }
            Err(error) => {
                self.status = Some(StatusMessage::error(error.to_string()));
                self.screen = Screen::List;
                self.details = None;
                Vec::new()
            }
        }
    }

    fn on_action(&mut self, result: Result<ActionOk, EngineError>) -> Vec<Effect> {
        match result {
            Ok(ActionOk::Added(outcome)) => {
                self.status = Some(StatusMessage::info(format!(
                    "added #{} {}",
                    outcome.id, outcome.name
                )));
                self.pending_select = Some(outcome.id);
                Vec::new()
            }
            Ok(ActionOk::Paused(id)) => {
                self.status = Some(StatusMessage::info(format!("paused #{id}")));
                Vec::new()
            }
            Ok(ActionOk::Resumed(id)) => {
                self.auto_paused.retain(|paused| *paused != id);
                self.status = Some(StatusMessage::info(format!("resumed #{id}")));
                Vec::new()
            }
            Ok(ActionOk::Removed {
                identifier,
                delete_files,
            }) => {
                let suffix = if delete_files { " and its data" } else { "" };
                self.status = Some(StatusMessage::info(format!("removed {identifier}{suffix}")));
                Vec::new()
            }
            Ok(ActionOk::FilesUpdated(id)) => vec![Effect::LoadDetails(id)],
            Err(error) => {
                self.status = Some(StatusMessage::error(error.to_string()));
                Vec::new()
            }
        }
    }

    fn on_paste(&mut self, text: &str) -> Vec<Effect> {
        let Some(Modal::Input(state)) = self.modal.as_mut() else {
            return Vec::new();
        };
        match sanitize_paste(text) {
            Ok(text) => state.insert_str(&text),
            Err(error) => state.set_error(error),
        }
        Vec::new()
    }

    fn on_add_failed(&mut self, source: &AddSource, error: String) -> Vec<Effect> {
        let mut state = InputState::new(source.display());
        state.set_error(error);
        self.modal = Some(Modal::Input(state));
        Vec::new()
    }

    fn on_tick(&mut self) -> Vec<Effect> {
        if self
            .status
            .as_ref()
            .is_some_and(|status| status.is_expired(Instant::now()))
        {
            self.status = None;
        }
        Vec::new()
    }

    fn on_key(&mut self, key: Key) -> Vec<Effect> {
        if key == Key::CtrlC {
            return vec![Effect::Quit];
        }
        if self.modal.is_some() {
            return self.on_modal_key(key);
        }
        match self.screen {
            Screen::List => self.on_list_key(key),
            Screen::Detail => self.on_detail_key(key),
        }
    }

    fn on_list_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Char('q') => vec![Effect::Quit],
            Key::Char('?') => {
                self.modal = Some(Modal::Help);
                Vec::new()
            }
            Key::Char('a') => {
                self.modal = Some(Modal::Input(InputState::default()));
                Vec::new()
            }
            Key::Char(' ' | 'p' | 'r') => self.pause_toggle(key),
            Key::Char('d') => {
                if let Some(torrent) = self.selected_torrent() {
                    self.modal = Some(Modal::Confirm(ConfirmState {
                        id: torrent.id,
                        name: torrent.name.clone(),
                    }));
                }
                Vec::new()
            }
            Key::Enter => {
                let Some(id) = self.selected_id() else {
                    return Vec::new();
                };
                self.screen = Screen::Detail;
                self.details = None;
                self.file_cursor = 0;
                vec![Effect::LoadDetails(id)]
            }
            Key::Up => self.move_selection(-1),
            Key::Down => self.move_selection(1),
            Key::PageUp => self.move_selection(-10),
            Key::PageDown => self.move_selection(10),
            Key::Home => {
                self.selected_index = 0;
                Vec::new()
            }
            Key::End => {
                self.selected_index = self.torrents.len().saturating_sub(1);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_detail_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Esc | Key::Backspace | Key::Char('q') => {
                self.screen = Screen::List;
                self.details = None;
                Vec::new()
            }
            Key::Char('?') => {
                self.modal = Some(Modal::Help);
                Vec::new()
            }
            Key::Char('p' | 'r') => self.pause_toggle(key),
            Key::Char(' ') => self.toggle_file(),
            Key::Up => self.move_file_cursor(-1),
            Key::Down => self.move_file_cursor(1),
            Key::PageUp => self.move_file_cursor(-10),
            Key::PageDown => self.move_file_cursor(10),
            Key::Home => {
                self.file_cursor = 0;
                Vec::new()
            }
            Key::End => {
                self.file_cursor = self.file_count().saturating_sub(1);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn pause_toggle(&mut self, key: Key) -> Vec<Effect> {
        let Some(torrent) = self.selected_torrent() else {
            return Vec::new();
        };
        match key {
            Key::Char('r') => vec![Effect::Resume(torrent.id)],
            Key::Char('p') => vec![Effect::Pause(torrent.id)],
            _ if torrent.status.is_active() => vec![Effect::Pause(torrent.id)],
            _ => vec![Effect::Resume(torrent.id)],
        }
    }

    fn toggle_file(&mut self) -> Vec<Effect> {
        let Some(details) = self.details.as_mut() else {
            return Vec::new();
        };
        let Some(file) = details.files.get_mut(self.file_cursor) else {
            return Vec::new();
        };
        file.included = !file.included;
        let id = details.view.id;
        let included = details
            .files
            .iter()
            .filter(|file| file.included)
            .map(|file| file.index)
            .collect();
        vec![Effect::SetIncludedFiles { id, included }]
    }

    fn move_selection(&mut self, delta: isize) -> Vec<Effect> {
        if self.torrents.is_empty() {
            return Vec::new();
        }
        let last = self.torrents.len().cast_signed() - 1;
        let next = (self.selected_index.cast_signed() + delta).clamp(0, last);
        self.selected_index = next.cast_unsigned();
        Vec::new()
    }

    fn move_file_cursor(&mut self, delta: isize) -> Vec<Effect> {
        let last = self.file_count().saturating_sub(1).cast_signed();
        let next = (self.file_cursor.cast_signed() + delta).clamp(0, last);
        self.file_cursor = next.cast_unsigned();
        Vec::new()
    }

    fn file_count(&self) -> usize {
        self.details
            .as_ref()
            .map_or(0, |details| details.files.len())
    }

    fn on_modal_key(&mut self, key: Key) -> Vec<Effect> {
        let Some(modal) = self.modal.take() else {
            return Vec::new();
        };
        let (next, effects) = match modal {
            Modal::Help => {
                let closes = matches!(key, Key::Esc | Key::Enter | Key::Char('?' | 'q'));
                (if closes { None } else { Some(Modal::Help) }, Vec::new())
            }
            Modal::Input(mut state) => match key {
                Key::Esc => (None, Vec::new()),
                Key::Enter => match AddSource::parse(state.value.trim(), self.home.as_deref()) {
                    Ok(source) => (None, vec![Effect::Add { source }]),
                    Err(error) => {
                        state.set_error(error.to_string());
                        (Some(Modal::Input(state)), Vec::new())
                    }
                },
                Key::Backspace => {
                    state.backspace();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::Delete => {
                    state.delete();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::Left => {
                    state.move_left();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::Right => {
                    state.move_right();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::Home => {
                    state.home();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::End => {
                    state.end();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::CtrlW => {
                    state.delete_word_back();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::CtrlU => {
                    state.clear();
                    (Some(Modal::Input(state)), Vec::new())
                }
                Key::Char(c) => {
                    state.insert_char(c);
                    (Some(Modal::Input(state)), Vec::new())
                }
                _ => (Some(Modal::Input(state)), Vec::new()),
            },
            Modal::Confirm(state) => match key {
                Key::Esc | Key::Char('n') => (None, Vec::new()),
                Key::Char('y') | Key::Enter => (
                    None,
                    vec![Effect::Remove {
                        id: state.id,
                        delete_files: false,
                    }],
                ),
                Key::Char('d') => (
                    None,
                    vec![Effect::Remove {
                        id: state.id,
                        delete_files: true,
                    }],
                ),
                _ => (Some(Modal::Confirm(state)), Vec::new()),
            },
        };
        self.modal = next;
        effects
    }
}

fn sanitize_paste(text: &str) -> Result<String, &'static str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("nothing to paste");
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err("paste contains more than one line");
    }
    if trimmed.chars().any(char::is_control) {
        return Err("paste contains control characters");
    }
    if trimmed.chars().count() > MAX_INPUT_LEN {
        return Err("pasted text is too long");
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileView, TorrentView};

    fn torrent(id: usize, name: &str, status: TorrentStatus) -> TorrentView {
        TorrentView {
            id: TorrentId(id),
            info_hash: format!("{id:08x}"),
            name: name.to_owned(),
            output_folder: "/dl".to_owned(),
            total_bytes: 1000,
            done_bytes: 500,
            uploaded_bytes: 100,
            download_speed: 10,
            upload_speed: 5,
            peers_live: 2,
            peers_connecting: 0,
            peers_seen: 9,
            status,
            error: None,
        }
    }

    fn app_with(torrents: Vec<TorrentView>) -> App {
        let mut app = App::new("balanced", true, None);
        app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView::default(),
            torrents,
        }));
        app
    }

    fn keys(app: &mut App, keys: &[Key]) -> Vec<Effect> {
        app.handle_event(AppEvent::Keys(keys.to_vec()))
    }

    #[test]
    fn moves_selection_within_bounds() {
        let mut app = app_with(vec![
            torrent(1, "a", TorrentStatus::Downloading),
            torrent(2, "b", TorrentStatus::Paused),
        ]);
        keys(&mut app, &[Key::Up]);
        assert_eq!(app.selected_id(), Some(TorrentId(1)));
        keys(&mut app, &[Key::Down, Key::Down, Key::Down]);
        assert_eq!(app.selected_id(), Some(TorrentId(2)));
        keys(&mut app, &[Key::Home]);
        assert_eq!(app.selected_id(), Some(TorrentId(1)));
        keys(&mut app, &[Key::End]);
        assert_eq!(app.selected_id(), Some(TorrentId(2)));
    }

    #[test]
    fn space_pauses_active_and_resumes_paused() {
        let mut app = app_with(vec![torrent(1, "a", TorrentStatus::Downloading)]);
        assert_eq!(
            keys(&mut app, &[Key::Char(' ')]),
            vec![Effect::Pause(TorrentId(1))]
        );
        app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView::default(),
            torrents: vec![torrent(1, "a", TorrentStatus::Paused)],
        }));
        assert_eq!(
            keys(&mut app, &[Key::Char(' ')]),
            vec![Effect::Resume(TorrentId(1))]
        );
    }

    #[test]
    fn q_quits_and_ctrl_c_quits_from_anywhere() {
        let mut app = app_with(vec![]);
        assert_eq!(keys(&mut app, &[Key::Char('q')]), vec![Effect::Quit]);
        assert_eq!(keys(&mut app, &[Key::CtrlC]), vec![Effect::Quit]);
    }

    #[test]
    fn selection_follows_torrent_across_removal() {
        let mut app = app_with(vec![
            torrent(1, "a", TorrentStatus::Downloading),
            torrent(2, "b", TorrentStatus::Downloading),
            torrent(3, "c", TorrentStatus::Downloading),
        ]);
        keys(&mut app, &[Key::Down]);
        assert_eq!(app.selected_id(), Some(TorrentId(2)));
        app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView::default(),
            torrents: vec![
                torrent(1, "a", TorrentStatus::Downloading),
                torrent(3, "c", TorrentStatus::Downloading),
            ],
        }));
        assert_eq!(app.selected_id(), Some(TorrentId(3)));
    }

    #[test]
    fn remove_requires_confirmation_and_can_delete_data() {
        let mut app = app_with(vec![torrent(1, "a", TorrentStatus::Downloading)]);
        keys(&mut app, &[Key::Char('d')]);
        assert!(matches!(app.modal, Some(Modal::Confirm(_))));
        keys(&mut app, &[Key::Esc]);
        assert!(app.modal.is_none());

        keys(&mut app, &[Key::Char('d'), Key::Char('d')]);
        assert_eq!(
            app.handle_event(AppEvent::Action(Ok(ActionOk::Removed {
                identifier: "a".to_owned(),
                delete_files: true,
            }))),
            vec![]
        );
        assert_eq!(
            keys(&mut app, &[Key::Char('d'), Key::Char('y')]),
            vec![Effect::Remove {
                id: TorrentId(1),
                delete_files: false
            }]
        );
    }

    #[test]
    fn add_dialog_classifies_sources() {
        let mut app = app_with(vec![]);
        keys(&mut app, &[Key::Char('a')]);
        for c in "magnet:?xt=urn:btih:aa".chars() {
            keys(&mut app, &[Key::Char(c)]);
        }
        let effects = keys(&mut app, &[Key::Enter]);
        assert_eq!(
            effects,
            vec![Effect::Add {
                source: AddSource::Magnet("magnet:?xt=urn:btih:aa".to_owned())
            }]
        );

        keys(&mut app, &[Key::Char('a'), Key::Enter]);
        assert!(app.modal.is_some());
        assert!(matches!(
            app.modal,
            Some(Modal::Input(InputState { ref error, .. })) if error.is_some()
        ));
    }

    #[test]
    fn added_torrent_becomes_selected_after_next_snapshot() {
        let mut app = app_with(vec![torrent(1, "a", TorrentStatus::Downloading)]);
        app.handle_event(AppEvent::Action(Ok(ActionOk::Added(AddOutcome {
            id: TorrentId(9),
            name: "new".to_owned(),
            info_hash: "ffffffff".to_owned(),
        }))));
        app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView::default(),
            torrents: vec![
                torrent(1, "a", TorrentStatus::Downloading),
                torrent(9, "new", TorrentStatus::Downloading),
            ],
        }));
        assert_eq!(app.selected_id(), Some(TorrentId(9)));
    }

    #[test]
    fn enters_and_leaves_detail_screen() {
        let mut app = app_with(vec![torrent(1, "a", TorrentStatus::Downloading)]);
        assert_eq!(
            keys(&mut app, &[Key::Enter]),
            vec![Effect::LoadDetails(TorrentId(1))]
        );
        assert_eq!(app.screen, Screen::Detail);

        app.handle_event(AppEvent::Details(Ok(TorrentDetails {
            view: torrent(1, "a", TorrentStatus::Downloading),
            files: vec![
                FileView {
                    index: 0,
                    name: "one.bin".to_owned(),
                    bytes: 100,
                    done_bytes: 100,
                    included: true,
                },
                FileView {
                    index: 1,
                    name: "two.bin".to_owned(),
                    bytes: 100,
                    done_bytes: 0,
                    included: true,
                },
            ],
        })));

        keys(&mut app, &[Key::Down, Key::Char(' ')]);
        let effects = app.handle_event(AppEvent::Action(Ok(ActionOk::FilesUpdated(TorrentId(1)))));
        assert_eq!(effects, vec![Effect::LoadDetails(TorrentId(1))]);
        let details = app.details.as_ref().expect("details loaded");
        assert!(!details.files[1].included);
        assert!(details.files[0].included);

        keys(&mut app, &[Key::Esc]);
        assert_eq!(app.screen, Screen::List);
    }

    #[test]
    fn auto_pauses_finished_torrents_when_seeding_is_off() {
        let mut app = App::new("private", false, None);
        let effects = app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView::default(),
            torrents: vec![torrent(1, "a", TorrentStatus::Seeding)],
        }));
        assert_eq!(effects, vec![Effect::Pause(TorrentId(1))]);

        let again = app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView::default(),
            torrents: vec![torrent(1, "a", TorrentStatus::Seeding)],
        }));
        assert_eq!(again, vec![]);
    }

    #[test]
    fn keeps_seeding_when_enabled() {
        let mut app = App::new("balanced", true, None);
        let effects = app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView::default(),
            torrents: vec![torrent(1, "a", TorrentStatus::Seeding)],
        }));
        assert_eq!(effects, vec![]);
    }

    #[test]
    fn error_action_becomes_visible_status() {
        let mut app = app_with(vec![]);
        app.handle_event(AppEvent::Action(Err(EngineError::Failed(
            "boom".to_owned(),
        ))));
        let status = app.status.as_ref().expect("status set");
        assert_eq!(status.level, StatusLevel::Error);
        assert_eq!(status.text, "boom");
    }

    #[test]
    fn tick_expires_old_status() {
        let mut app = app_with(vec![]);
        app.status = Some(StatusMessage {
            text: "old".to_owned(),
            level: StatusLevel::Info,
            created: Instant::now()
                .checked_sub(Duration::from_secs(30))
                .expect("instant in the past"),
        });
        keys(&mut app, &[Key::Other]);
        app.handle_event(AppEvent::Tick);
        assert!(app.status.is_none());
    }

    #[test]
    fn help_modal_closes_with_escape() {
        let mut app = app_with(vec![]);
        keys(&mut app, &[Key::Char('?')]);
        assert_eq!(app.modal, Some(Modal::Help));
        keys(&mut app, &[Key::Esc]);
        assert_eq!(app.modal, None);
    }

    fn input_app(home: Option<&str>) -> App {
        let mut app = App::new("balanced", true, home.map(PathBuf::from));
        keys(&mut app, &[Key::Char('a')]);
        app
    }

    fn input_value(app: &App) -> &InputState {
        let Some(Modal::Input(state)) = app.modal.as_ref() else {
            panic!("expected input modal");
        };
        state
    }

    #[test]
    fn input_cursor_inserts_and_deletes_in_the_middle() {
        let mut app = input_app(None);
        for c in "abcd".chars() {
            keys(&mut app, &[Key::Char(c)]);
        }
        keys(&mut app, &[Key::Left, Key::Left, Key::Char('X')]);
        assert_eq!(input_value(&app).value, "abXcd");
        assert_eq!(input_value(&app).cursor, 3);

        keys(&mut app, &[Key::Backspace]);
        assert_eq!(input_value(&app).value, "abcd");
        keys(&mut app, &[Key::Delete, Key::Home, Key::Delete]);
        assert_eq!(input_value(&app).value, "bd");
        keys(&mut app, &[Key::End, Key::Delete]);
        assert_eq!(input_value(&app).value, "bd");
    }

    #[test]
    fn input_ctrl_w_deletes_previous_word() {
        let mut app = input_app(None);
        for c in "/tmp/some file.torrent".chars() {
            keys(&mut app, &[Key::Char(c)]);
        }
        keys(&mut app, &[Key::CtrlW]);
        assert_eq!(input_value(&app).value, "/tmp/some ");
        keys(&mut app, &[Key::CtrlU]);
        assert_eq!(input_value(&app).value, "");
    }

    #[test]
    fn input_cursor_is_unicode_safe() {
        let mut app = input_app(None);
        for c in "áéí".chars() {
            keys(&mut app, &[Key::Char(c)]);
        }
        keys(&mut app, &[Key::Left, Key::Char('x')]);
        assert_eq!(input_value(&app).value, "áéxí");
        assert_eq!(input_value(&app).cursor, 3);
    }

    #[test]
    fn paste_inserts_at_cursor_and_validates_content() {
        let mut app = input_app(None);
        for c in "ab".chars() {
            keys(&mut app, &[Key::Char(c)]);
        }
        keys(&mut app, &[Key::Left]);
        app.handle_event(AppEvent::Paste("XY".to_owned()));
        assert_eq!(input_value(&app).value, "aXYb");

        app.handle_event(AppEvent::Paste("line1\nline2".to_owned()));
        assert_eq!(
            input_value(&app).error.as_deref(),
            Some("paste contains more than one line")
        );

        app.handle_event(AppEvent::Paste("ok\n".to_owned()));
        assert_eq!(input_value(&app).value, "aXYokb");
    }

    #[test]
    fn enter_normalizes_tilde_and_quotes() {
        let mut app = input_app(Some("/home/user"));
        app.handle_event(AppEvent::Paste("\"~/Downloads/x.torrent\"".to_owned()));
        assert_eq!(
            keys(&mut app, &[Key::Enter]),
            vec![Effect::Add {
                source: AddSource::File(PathBuf::from("/home/user/Downloads/x.torrent"))
            }]
        );
    }

    #[test]
    fn add_failure_reopens_the_dialog_with_the_error() {
        let mut app = app_with(vec![]);
        app.handle_event(AppEvent::AddFailed {
            source: AddSource::File(PathBuf::from("/missing.torrent")),
            error: "file not found: /missing.torrent".to_owned(),
        });
        let state = input_value(&app);
        assert_eq!(state.value, "/missing.torrent");
        assert_eq!(state.cursor, state.value.chars().count());
        assert_eq!(
            state.error.as_deref(),
            Some("file not found: /missing.torrent")
        );
    }
}
