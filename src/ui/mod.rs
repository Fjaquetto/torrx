mod theme;

use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, Wrap},
};

use crate::{
    app::{App, ConfirmState, InputState, Modal, Screen},
    domain::{FileView, TorrentView},
    format,
};

use theme as t;

const MIN_WIDTH: u16 = 76;
const MIN_HEIGHT: u16 = 12;
const HEADER_HEIGHT: u16 = 6;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        render_too_small(frame, area);
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(HEADER_HEIGHT),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(area);

    render_header(frame, header, app);
    match app.screen {
        Screen::List => render_list(frame, body, app),
        Screen::Detail => render_detail(frame, body, app),
    }
    render_footer(frame, footer, app);

    match &app.modal {
        Some(Modal::Help) => render_help(frame, area),
        Some(Modal::Input(input)) => render_input(frame, area, input),
        Some(Modal::Confirm(confirm)) => render_confirm(frame, area, confirm),
        None => {}
    }
}

fn render_too_small(frame: &mut Frame, area: Rect) {
    let text = format!(
        "terminal too small\nneed at least {MIN_WIDTH}x{MIN_HEIGHT}, have {}x{}",
        area.width, area.height
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::new().fg(t::ERROR))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let global = &app.global;
    let discovery = if global.dht_enabled {
        "dht on"
    } else {
        "dht off"
    };
    let listening = match global.listen_port {
        Some(port) => format!("port {port}"),
        None => "not listening".to_owned(),
    };
    let title = format!(" torrx · {} · {discovery} · {listening} ", app.profile);

    let block = Block::bordered()
        .title(title)
        .border_style(Style::new().fg(t::MUTED));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).areas(inner);
    let speeds = Line::from(vec![
        Span::styled("↓ ", Style::new().fg(t::DOWNLOAD)),
        Span::styled(
            format::speed(global.download_speed),
            Style::new().fg(t::DOWNLOAD),
        ),
        Span::raw("   "),
        Span::styled("↑ ", Style::new().fg(t::UPLOAD)),
        Span::styled(
            format::speed(global.upload_speed),
            Style::new().fg(t::UPLOAD),
        ),
    ]);
    let totals = Line::from(format!(
        "session ↓ {}  ↑ {}",
        format::bytes(global.downloaded_bytes),
        format::bytes(global.uploaded_bytes)
    ))
    .style(Style::new().fg(t::TEXT));
    let peers = Line::from(format!(
        "peers {} · uptime {}",
        global.peers_live,
        format::uptime(Duration::from_secs(global.uptime_seconds))
    ))
    .style(Style::new().fg(t::MUTED));

    frame.render_widget(Paragraph::new(vec![speeds, totals, peers]), left);

    let [caption, chart] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(right);
    frame.render_widget(
        Paragraph::new(Line::from("↓ last 120s").style(Style::new().fg(t::MUTED))),
        caption,
    );
    let sparkline = Sparkline::default()
        .data(app.history.down().iter().copied())
        .max(app.history.max().max(1))
        .style(Style::new().fg(t::DOWNLOAD));
    frame.render_widget(sparkline, chart);
}

fn render_list(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(t::MUTED))
        .title(" torrents ");

    if app.torrents.is_empty() {
        frame.render_widget(
            Paragraph::new(
                "no torrents yet — press `a` to add a magnet link, URL or .torrent file",
            )
            .style(Style::new().fg(t::MUTED))
            .block(block)
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let widths = column_widths(area.width);
    let rows: Vec<Row> = app
        .torrents
        .iter()
        .enumerate()
        .map(|(index, torrent)| torrent_row(torrent, index == app.selected_index, &widths))
        .collect();

    frame.render_widget(
        Table::new(rows, widths.constraints())
            .header(header_row())
            .column_spacing(1)
            .block(block),
        area,
    );
}

fn header_row() -> Row<'static> {
    let style = Style::new().fg(t::MUTED).add_modifier(Modifier::BOLD);
    let cell = |label: &'static str| Cell::from(label).style(style);
    Row::new(vec![
        cell(""),
        cell("st"),
        cell("name"),
        cell("size"),
        cell("progress"),
        cell("done"),
        cell("down"),
        cell("up"),
        cell("eta"),
        cell("peers"),
        cell("ratio"),
    ])
}

fn torrent_row(torrent: &TorrentView, selected: bool, widths: &ColumnWidths) -> Row<'static> {
    let marker = if selected { "▌" } else { " " };
    let name_style = if selected {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };

    let mut row = Row::new(vec![
        Cell::from(marker).style(Style::new().fg(t::ACCENT)),
        Cell::from(torrent.status.label()).style(t::status_style(torrent.status)),
        Cell::from(format::truncate(&torrent.name, widths.name)).style(name_style),
        Cell::from(format::bytes(torrent.total_bytes)).style(Style::new().fg(t::TEXT)),
        Cell::from(Line::from(bar_spans(
            torrent.fraction(),
            widths.bar,
            t::status_color(torrent.status),
        ))),
        Cell::from(format::percent(torrent.fraction())).style(Style::new().fg(t::TEXT)),
        Cell::from(format::speed(torrent.download_speed)).style(Style::new().fg(t::DOWNLOAD)),
        Cell::from(format::speed(torrent.upload_speed)).style(Style::new().fg(t::UPLOAD)),
        Cell::from(format::eta(torrent.eta())).style(Style::new().fg(t::TEXT)),
        Cell::from(format!("{}", torrent.peers_live + torrent.peers_connecting))
            .style(Style::new().fg(t::TEXT)),
        Cell::from(format::ratio(torrent.uploaded_bytes, torrent.done_bytes))
            .style(Style::new().fg(t::TEXT)),
    ]);

    if selected {
        row = row.style(Style::new().bg(t::SELECTION_BG));
    }
    row
}

struct ColumnWidths {
    name: usize,
    bar: usize,
}

impl ColumnWidths {
    fn constraints(&self) -> Vec<Constraint> {
        vec![
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Length(self.name as u16),
            Constraint::Length(9),
            Constraint::Length(self.bar as u16),
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Length(5),
        ]
    }
}

fn column_widths(total: u16) -> ColumnWidths {
    const FIXED: u16 = 1 + 6 + 9 + 6 + 12 + 12 + 7 + 5 + 5 + 10;
    const BAR_MAX: u16 = 32;
    const NAME_RESERVE: u16 = 24;

    let spare = total.saturating_sub(FIXED);
    let bar = spare.saturating_sub(NAME_RESERVE).clamp(8, BAR_MAX);
    let name = spare.saturating_sub(bar).clamp(16, 44);
    ColumnWidths {
        name: name as usize,
        bar: bar as usize,
    }
}

fn bar_spans(fraction: f64, width: usize, color: ratatui::style::Color) -> Vec<Span<'static>> {
    let width = width.max(1);
    let filled = (fraction.clamp(0.0, 1.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    vec![
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled("░".repeat(width - filled), Style::new().fg(t::MUTED)),
    ]
}

fn render_detail(frame: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)]).areas(area);
    render_files(frame, left, app);
    render_torrent_info(frame, right, app);
}

fn render_files(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(t::MUTED))
        .title(" files ");

    let Some(details) = app.details.as_ref() else {
        frame.render_widget(
            Paragraph::new("loading file list…")
                .style(Style::new().fg(t::MUTED))
                .block(block),
            area,
        );
        return;
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let name_width = file_name_width(inner.width);
    let rows: Vec<Row> = details
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| file_row(file, index == app.file_cursor, name_width))
        .collect();

    frame.render_widget(
        Table::new(rows, file_constraints(inner.width))
            .header(file_header_row())
            .column_spacing(1),
        inner,
    );
}

fn file_header_row() -> Row<'static> {
    let style = Style::new().fg(t::MUTED).add_modifier(Modifier::BOLD);
    Row::new(vec![
        Cell::from("").style(style),
        Cell::from("name").style(style),
        Cell::from("progress").style(style),
        Cell::from("done").style(style),
    ])
}

fn file_name_width(width: u16) -> usize {
    (width as usize).saturating_sub(24).max(12)
}

fn file_constraints(width: u16) -> Vec<Constraint> {
    vec![
        Constraint::Length(3),
        Constraint::Length(file_name_width(width) as u16),
        Constraint::Length(10),
        Constraint::Length(7),
    ]
}

fn file_row(file: &FileView, selected: bool, name_width: usize) -> Row<'static> {
    let mark = if file.included { "[x]" } else { "[ ]" };
    let style = if selected {
        Style::new()
            .bg(t::SELECTION_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    Row::new(vec![
        Cell::from(mark).style(Style::new().fg(if file.included { t::INFO } else { t::MUTED })),
        Cell::from(format::truncate(&file.name, name_width)).style(Style::new().fg(t::TEXT)),
        Cell::from(Line::from(bar_spans(file.fraction(), 10, t::DOWNLOAD))),
        Cell::from(format::percent(file.fraction())).style(Style::new().fg(t::TEXT)),
    ])
    .style(style)
}

fn render_torrent_info(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(t::MUTED))
        .title(" torrent ");

    let Some(torrent) = app
        .details
        .as_ref()
        .map(|details| &details.view)
        .or_else(|| app.selected_torrent())
    else {
        frame.render_widget(block, area);
        return;
    };

    let label = Style::new().fg(t::MUTED);
    let mut lines = vec![
        Line::from(Span::styled(torrent.name.clone(), Style::new().bold())),
        Line::from(vec![
            Span::styled("status   ", label),
            Span::styled(torrent.status.label(), t::status_style(torrent.status)),
        ]),
        Line::from(vec![
            Span::styled("size     ", label),
            Span::raw(format::bytes(torrent.total_bytes)),
        ]),
        Line::from(vec![
            Span::styled("done     ", label),
            Span::raw(format!(
                "{} ({})",
                format::bytes(torrent.done_bytes),
                format::percent(torrent.fraction())
            )),
        ]),
        Line::from(vec![
            Span::styled("down     ", label),
            Span::styled(
                format::speed(torrent.download_speed),
                Style::new().fg(t::DOWNLOAD),
            ),
        ]),
        Line::from(vec![
            Span::styled("up       ", label),
            Span::styled(
                format::speed(torrent.upload_speed),
                Style::new().fg(t::UPLOAD),
            ),
        ]),
        Line::from(vec![
            Span::styled("peers    ", label),
            Span::raw(format!(
                "{} live, {} connecting, {} seen",
                torrent.peers_live, torrent.peers_connecting, torrent.peers_seen
            )),
        ]),
        Line::from(vec![
            Span::styled("ratio    ", label),
            Span::raw(format::ratio(torrent.uploaded_bytes, torrent.done_bytes)),
        ]),
        Line::from(vec![
            Span::styled("hash     ", label),
            Span::raw(torrent.info_hash.clone()),
        ]),
        Line::from(vec![
            Span::styled("folder   ", label),
            Span::raw(torrent.output_folder.clone()),
        ]),
    ];
    if let Some(error) = &torrent.error {
        lines.push(Line::from(vec![
            Span::styled("error    ", label),
            Span::styled(error.clone(), Style::new().fg(t::ERROR)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from("space toggles the file under the cursor · esc goes back")
            .style(Style::new().fg(t::MUTED)),
    );

    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(status) = app.status.as_ref().filter(|_| app.status_is_visible()) {
        let color = match status.level {
            crate::app::StatusLevel::Info => t::INFO,
            crate::app::StatusLevel::Error => t::ERROR,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {}", status.text),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ))),
            area,
        );
        return;
    }

    let keys = match app.screen {
        Screen::List => " q quit · a add · space pause/resume · d remove · enter details · ? help",
        Screen::Detail => " esc back · space toggle file · ↑/↓ move · ? help · q quit",
    };
    frame.render_widget(
        Paragraph::new(Line::from(keys).style(Style::new().fg(t::MUTED))),
        area,
    );
}

fn render_help(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(Span::styled(
            "torrx keys",
            Style::new().fg(t::ACCENT).bold(),
        )),
        Line::from(""),
        Line::from("a            add a magnet link, URL or .torrent path"),
        Line::from("space / p    pause, resume when paused"),
        Line::from("r            resume the selected torrent"),
        Line::from("d            remove the selected torrent"),
        Line::from("enter        open torrent details"),
        Line::from("↑/↓ PgUp/Dn  move through the list"),
        Line::from("?            toggle this help"),
        Line::from("q / ctrl-c   quit"),
        Line::from("").style(Style::new()),
        Line::from("uploads run while torrx is open; keeping the window open seeds.")
            .style(Style::new().fg(t::MUTED)),
    ];
    render_modal_floating(frame, area, 64, 14, " help ", Paragraph::new(text));
}

fn render_input(frame: &mut Frame, area: Rect, input: &InputState) {
    let mut lines = vec![
        Line::from(Span::styled(
            "add torrent",
            Style::new().fg(t::ACCENT).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("> "),
            Span::styled(input.value.clone(), Style::new().fg(t::TEXT)),
            Span::styled("█", Style::new().fg(t::ACCENT)),
        ]),
    ];
    if let Some(error) = &input.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::new().fg(t::ERROR),
        )));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from("enter to add · esc to cancel".to_owned()).style(Style::new().fg(t::MUTED)),
    );
    render_modal_floating(
        frame,
        area,
        70,
        8,
        " add torrent ",
        Paragraph::new(lines).wrap(Wrap { trim: false }),
    );
}

fn render_confirm(frame: &mut Frame, area: Rect, confirm: &ConfirmState) {
    let lines = vec![
        Line::from(Span::styled(
            "remove torrent?",
            Style::new().fg(t::ERROR).bold(),
        )),
        Line::from(""),
        Line::from(format::truncate(&confirm.name, 60)),
        Line::from(""),
        Line::from("y / enter  remove, keep files"),
        Line::from("d          remove and delete the downloaded data"),
        Line::from("n / esc    cancel"),
    ];
    render_modal_floating(frame, area, 64, 9, " confirm ", Paragraph::new(lines));
}

fn render_modal_floating(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    paragraph: Paragraph<'_>,
) {
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        paragraph.block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(t::ACCENT))
                .title(title.to_owned()),
        ),
        popup,
    );
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        app::{AppEvent, Key},
        domain::{GlobalView, Snapshot, TorrentDetails, TorrentStatus, TorrentView},
    };

    fn torrent(id: usize, name: &str, status: TorrentStatus) -> TorrentView {
        TorrentView {
            id: crate::domain::TorrentId(id),
            info_hash: "aabbccdd".to_owned(),
            name: name.to_owned(),
            output_folder: "/home/user/downloads".to_owned(),
            total_bytes: 1024 * 1024,
            done_bytes: 512 * 1024,
            uploaded_bytes: 256 * 1024,
            download_speed: 1024 * 1024,
            upload_speed: 2048,
            peers_live: 4,
            peers_connecting: 1,
            peers_seen: 20,
            status,
            error: None,
        }
    }

    fn app() -> App {
        let mut app = App::new("balanced", true);
        app.handle_event(AppEvent::Snapshot(Snapshot {
            global: GlobalView {
                download_speed: 1024 * 1024,
                upload_speed: 4096,
                downloaded_bytes: 10 * 1024 * 1024,
                uploaded_bytes: 1024 * 1024,
                peers_live: 5,
                uptime_seconds: 3661,
                dht_enabled: true,
                listen_port: Some(51413),
            },
            torrents: vec![
                torrent(1, "debian-13.iso", TorrentStatus::Downloading),
                torrent(2, "linux-mint.iso", TorrentStatus::Seeding),
            ],
        }));
        app
    }

    fn lines(app: &App, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, app))
            .expect("renders frame");
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| {
                        buffer
                            .cell((x, y))
                            .map_or(" ", ratatui::buffer::Cell::symbol)
                    })
                    .collect()
            })
            .collect()
    }

    fn contains(lines: &[String], needle: &str) -> bool {
        lines.iter().any(|line| line.contains(needle))
    }

    #[test]
    fn renders_empty_state_hint() {
        let app = App::new("balanced", true);
        let screen = lines(&app, 120, 30);
        assert!(contains(&screen, "no torrents yet"));
    }

    #[test]
    fn renders_header_torrents_and_footer() {
        let app = app();
        let screen = lines(&app, 120, 30);
        assert!(contains(&screen, "torrx · balanced · dht on · port 51413"));
        assert!(contains(&screen, "debian-13.iso"));
        assert!(contains(&screen, "linux-mint.iso"));
        assert!(contains(&screen, "down"));
        assert!(contains(&screen, "seed"));
        assert!(contains(&screen, "1.00 MiB/s"));
        assert!(contains(&screen, "50.0%"));
        assert!(contains(&screen, "q quit"));
    }

    #[test]
    fn marks_selected_row() {
        let app = app();
        let screen = lines(&app, 120, 30);
        let selected = screen
            .iter()
            .find(|line| line.contains("debian-13.iso"))
            .expect("first row rendered");
        assert!(selected.contains("▌"));
        let other = screen
            .iter()
            .find(|line| line.contains("linux-mint.iso"))
            .expect("second row rendered");
        assert!(!other.contains("▌"));
    }

    #[test]
    fn renders_input_modal() {
        let mut app = app();
        app.handle_event(AppEvent::Keys(vec![Key::Char('a')]));
        let screen = lines(&app, 120, 30);
        assert!(contains(&screen, "add torrent"));
        assert!(contains(&screen, "esc to cancel"));
    }

    #[test]
    fn renders_confirm_modal_with_delete_hint() {
        let mut app = app();
        app.handle_event(AppEvent::Keys(vec![Key::Char('d')]));
        let screen = lines(&app, 120, 30);
        assert!(contains(&screen, "remove torrent?"));
        assert!(contains(&screen, "delete the downloaded data"));
    }

    #[test]
    fn renders_detail_with_files_and_info() {
        let mut app = app();
        app.handle_event(AppEvent::Keys(vec![Key::Enter]));
        app.handle_event(AppEvent::Details(Ok(TorrentDetails {
            view: torrent(1, "debian-13.iso", TorrentStatus::Downloading),
            files: vec![
                FileView {
                    index: 0,
                    name: "debian-13.iso".to_owned(),
                    bytes: 1024,
                    done_bytes: 512,
                    included: true,
                },
                FileView {
                    index: 1,
                    name: "readme.txt".to_owned(),
                    bytes: 1024,
                    done_bytes: 0,
                    included: false,
                },
            ],
        })));

        let screen = lines(&app, 120, 30);
        assert!(contains(&screen, "files"));
        assert!(contains(&screen, "[x]"));
        assert!(contains(&screen, "[ ]"));
        assert!(contains(&screen, "readme.txt"));
        assert!(contains(&screen, "aabbccdd"));
    }

    #[test]
    fn renders_help_overlay() {
        let mut app = app();
        app.handle_event(AppEvent::Keys(vec![Key::Char('?')]));
        let screen = lines(&app, 120, 30);
        assert!(contains(&screen, "torrx keys"));
        assert!(contains(&screen, "ctrl-c"));
    }

    #[test]
    fn renders_small_terminal_notice() {
        let app = app();
        let screen = lines(&app, 40, 8);
        assert!(contains(&screen, "terminal too small"));
    }
}
