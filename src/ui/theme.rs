use ratatui::style::{Color, Modifier, Style};

use crate::domain::TorrentStatus;

pub const ACCENT: Color = Color::Cyan;
pub const DOWNLOAD: Color = Color::Cyan;
pub const UPLOAD: Color = Color::Magenta;
pub const MUTED: Color = Color::DarkGray;
pub const TEXT: Color = Color::Gray;
pub const SELECTION_BG: Color = Color::Rgb(28, 36, 52);
pub const INFO: Color = Color::Green;
pub const ERROR: Color = Color::Red;

pub fn status_color(status: TorrentStatus) -> Color {
    match status {
        TorrentStatus::Downloading => Color::Cyan,
        TorrentStatus::Seeding => Color::Green,
        TorrentStatus::Paused => Color::Yellow,
        TorrentStatus::Error => Color::Red,
        TorrentStatus::Initializing => Color::Blue,
    }
}

pub fn status_style(status: TorrentStatus) -> Style {
    Style::new()
        .fg(status_color(status))
        .add_modifier(Modifier::BOLD)
}
