use super::{Command, Modal, Tui};
use cartridge_app::{files::plain, value_text, Page, Platform, SUPPORT, TERMINAL_MIN, VERSION};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

fn ink() -> Color {
    Color::Rgb(109, 211, 181)
}
fn themed_ink(state: &Tui) -> Color {
    if state.app.settings.color_scheme == 2 {
        Color::Rgb(20, 98, 78)
    } else {
        ink()
    }
}
fn panel(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .border_style(Style::default().fg(Color::DarkGray))
}
fn paragraph(frame: &mut Frame, area: Rect, title: &str, content: String) {
    frame.render_widget(
        Paragraph::new(plain(&content))
            .wrap(Wrap { trim: false })
            .block(panel(title)),
        area,
    );
}
fn button(
    frame: &mut Frame,
    state: &mut Tui,
    area: Rect,
    label: &str,
    command: Command,
    enabled: bool,
) {
    let focus = state.hits.len() == state.focus;
    let style = if !enabled {
        Style::default().fg(Color::DarkGray)
    } else if focus {
        Style::default()
            .fg(Color::Black)
            .bg(ink())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(themed_ink(state))
    };
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(style)
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
    if enabled {
        state.hits.push((area, command));
    }
}
pub fn draw(frame: &mut Frame, state: &mut Tui) {
    state.hits.clear();
    let area = frame.area();
    let light = state.app.settings.color_scheme == 2;
    frame.render_widget(
        Block::default().style(
            Style::default()
                .bg(if light {
                    Color::Rgb(239, 242, 238)
                } else {
                    Color::Rgb(18, 23, 30)
                })
                .fg(if light {
                    Color::Rgb(22, 29, 38)
                } else {
                    Color::Rgb(222, 230, 235)
                }),
        ),
        area,
    );
    if area.width < TERMINAL_MIN.0 || area.height < TERMINAL_MIN.1 {
        paragraph(frame, area, "Cartridge Studio", format!("\nWindow too small\n\nResize to at least {} columns × {} rows.\nCurrent size: {} × {}\n\n{}\n\n{}", TERMINAL_MIN.0, TERMINAL_MIN.1, area.width, area.height, state.app.status, if state.app.busy() { "The operation continues. Keep USB connected. X: stop safely." } else { "Your workspace is preserved. Q: quit." }));
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            " CARTRIDGE STUDIO  /  {VERSION}                                      {}",
            if state.app.connected {
                "● Reader connected"
            } else {
                "○ Reader not ready"
            }
        ))
        .style(
            Style::default()
                .fg(themed_ink(state))
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM)),
        rows[0],
    );
    let columns = Layout::horizontal([Constraint::Length(22), Constraint::Min(1)]).split(rows[1]);
    let nav = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(columns[0]);
    for (i, (label, command)) in [
        ("1  NES (72-pin)", Command::Platform(Platform::Nes)),
        ("2  Famicom (60-pin)", Command::Platform(Platform::Famicom)),
        ("3  Game Boy / Color", Command::Platform(Platform::GameBoy)),
        ("4  Game Boy Advance", Command::Platform(Platform::Gba)),
        ("5  Backup history", Command::Page(Page::History)),
        ("6  Supported boards", Command::Page(Page::Support)),
        ("7  Preferences", Command::Page(Page::Settings)),
        ("Ctrl+O  Load ROM", Command::Load),
    ]
    .into_iter()
    .enumerate()
    {
        button(frame, state, nav[i], label, command, !state.app.busy());
    }
    button(frame, state, nav[9], "?  Field guide", Command::Help, true);
    match state.app.page {
        Page::Workspace => workspace(frame, state, columns[1]),
        Page::Support => frame.render_widget(Paragraph::new(SUPPORT).wrap(Wrap { trim: false }).scroll((state.support_scroll, 0)).block(panel("Supported cartridges · arrows / PageUp / PageDown")), columns[1]),
        Page::Settings => paragraph(frame, columns[1], "Preferences", format!("\nU  Reader: {} (F5 to connect)\n{}\n\nD  Double read: {}\n\nS  Strict Game Boy checksum checks: {}\n\nG  GBA ROM size: {} (header checks always apply)\n\nA  Download artwork after reads: {}\n\nT  Appearance: {}\n\nF  Change library folder:\n{}\n\nOptions apply to ordinary reads. Backups and write verification always use mandatory checks.\n\nSettings are shared with the desktop. No Python, GTK or browser runtime is required.", state.app.settings.reader, state.app.reader_note(), yes(state.app.settings.double_read), yes(state.app.settings.strict_checksum), state.app.settings.gba_size, yes(state.app.settings.download_artwork), if light { "Light" } else { "Dark" }, state.app.settings.data_directory.display())),
        Page::History => {
            let items = state.app.history.iter().map(|h| ListItem::new(format!("{} · {}\n  {}", value_text(&h.report["operation"]), value_text(&h.report["status"]), h.path.parent().unwrap().file_name().unwrap_or_default().to_string_lossy()))).collect::<Vec<_>>();
            let items = if items.is_empty() { vec![ListItem::new("Your first backup will appear here.")] } else { items };
            let mut selection = ListState::default().with_selected(Some(state.history_selected));
            frame.render_stateful_widget(List::new(items).block(panel("Backup history · Enter: report · O: load ROM")).highlight_style(Style::default().fg(Color::Black).bg(ink())), columns[1], &mut selection);
        },
    }
    frame.render_widget(Paragraph::new(" U Reader    Tab / Enter Navigate    H Checksums    L  Activity    E  Error details    X  Stop    Q  Quit").block(Block::default().borders(Borders::TOP)).style(Style::default().fg(Color::Gray)), rows[2]);
    if state.app.review.is_some() {
        review(frame, state);
    } else if let Some(modal) = state.modal.clone() {
        modal_view(frame, state, &modal);
    }
    if state.focus >= state.hits.len() {
        state.focus = 0;
    }
}
fn yes(b: bool) -> &'static str {
    if b {
        "On"
    } else {
        "Off"
    }
}
fn workspace(frame: &mut Frame, state: &mut Tui, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Min(15),
        Constraint::Length(3),
        Constraint::Length(6),
    ])
    .split(area);
    let cols =
        Layout::horizontal([Constraint::Percentage(53), Constraint::Percentage(47)]).split(rows[0]);
    let left = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(5),
        Constraint::Length(4),
    ])
    .split(cols[0]);
    let right = Layout::vertical([Constraint::Min(8), Constraint::Length(7)]).split(cols[1]);
    paragraph(
        frame,
        left[0],
        &format!("{} · M: change board · P: detect", state.app.platform),
        format!(
            "{}\n{}",
            if state.app.platform == Platform::Gba {
                format!("G: ROM size · {}", state.app.settings.gba_size)
            } else {
                state.app.profile.name.to_string()
            },
            state.app.board_text()
        ),
    );
    let rom = state
        .app
        .source
        .as_ref()
        .map(|s| {
            format!(
                "{}\n{} · {} KiB · {}\n{}\n{}",
                s.title(),
                value_text(&s.info["format"]),
                s.info["bytes"].as_u64().unwrap_or(0) / 1024,
                value_text(&s.info["mapper"]),
                value_text(&s.info["checksum"]),
                s.path.display()
            )
        })
        .unwrap_or_else(|| {
            "No ROM loaded\n\nCtrl+O to browse, or R to read a cartridge.\n.gb  .gbc  .gba  .nes"
                .into()
        });
    paragraph(frame, left[1], "Loaded ROM", rom);
    paragraph(
        frame,
        left[2],
        "Read options",
        format!(
            "D Double read: {}   S Strict GB: {}\nA Download artwork: {}",
            yes(state.app.settings.double_read),
            yes(state.app.settings.strict_checksum),
            yes(state.app.settings.download_artwork)
        ),
    );
    paragraph(
        frame,
        right[0],
        "Game identity · exact catalog match",
        state.app.game_text(),
    );
    let hashes = state
        .app
        .source
        .as_ref()
        .map(|s| {
            format!(
                "CRC32   {}\nSHA-1   {}\nSHA-256\n{}",
                value_text(&s.info["hashes"]["crc32"]),
                value_text(&s.info["hashes"]["sha1"]),
                value_text(&s.info["hashes"]["sha256"])
            )
        })
        .unwrap_or_else(|| {
            "Checksums appear after reading or loading a ROM.\n\nH to compare an expected hash."
                .into()
        });
    paragraph(
        frame,
        right[1],
        "File checksums · H: compare / export",
        hashes,
    );
    let buttons = Layout::horizontal([Constraint::Ratio(1, 7); 7]).split(rows[1]);
    for (i, (label, action)) in [
        ("F5 USB", "doctor"),
        ("R Read", "read"),
        ("B Backup", "backup"),
        ("V Verify", "verify"),
        ("C Check", "check"),
        ("W Write", "write"),
        ("⇧W Wipe", "wipe"),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            frame,
            state,
            buttons[i],
            label,
            Command::Action(action),
            state.app.can(action),
        );
    }
    let status = state
        .app
        .error
        .as_ref()
        .map(|e| {
            format!(
                "{}\n{}\nE: Details and recovery information",
                e.message, e.action
            )
        })
        .unwrap_or_else(|| {
            format!(
                "{}\n{}\n{}",
                state.app.status,
                state.app.reader,
                state
                    .app
                    .directory
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| state.app.write_note().into())
            )
        });
    paragraph(
        frame,
        rows[2],
        if state.app.error.is_some() {
            "Action needed"
        } else if state.app.busy() {
            "Operation in progress · X to stop"
        } else {
            "Status"
        },
        status,
    );
}
fn center(frame: &mut Frame, title: &str) -> Rect {
    let a = frame.area();
    let width = a.width.saturating_sub(12).min(108);
    let height = a.height.saturating_sub(4).min(32);
    let area = Rect::new(
        a.x + (a.width - width) / 2,
        a.y + (a.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        panel(title)
            .style(Style::default().bg(Color::Rgb(25, 32, 42)).fg(Color::White))
            .border_style(Style::default().fg(ink())),
        area,
    );
    Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2)
}
fn review(frame: &mut Frame, state: &Tui) {
    let review = state.app.review.as_ref().unwrap();
    let area = center(frame, "Review cartridge change");
    let rows = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(plain(&review.text())).wrap(Wrap { trim: false }),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(review.word.as_str())
            .block(panel(&format!("Type {}", review.action().to_uppercase()))),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(if review.ready() {
            "Esc: Cancel                         Enter: Confirm cartridge change"
        } else {
            "Esc: Cancel                         Confirmation is disabled until the word matches"
        })
        .style(Style::default().fg(ink())),
        rows[2],
    );
}
fn modal_view(frame: &mut Frame, state: &Tui, modal: &Modal) {
    match modal {
        Modal::Text {
            title,
            content,
            scroll,
        } => {
            let area = center(frame, &format!("{title} · Esc to close"));
            frame.render_widget(
                Paragraph::new(plain(content))
                    .wrap(Wrap { trim: false })
                    .scroll((*scroll, 0)),
                area,
            );
        }
        Modal::Files {
            browser,
            selected,
            editing,
        } => {
            let area = center(frame, "Load ROM · Tab: path/list · Esc: cancel");
            let rows = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);
            frame.render_widget(
                Paragraph::new(plain(&browser.path)).block(panel(if *editing {
                    "Path · Ctrl+U: clear · Enter: open"
                } else {
                    "Path"
                })),
                rows[0],
            );
            let items = browser
                .entries
                .iter()
                .map(|e| ListItem::new(plain(&e.label())))
                .collect::<Vec<_>>();
            let mut selection =
                ListState::default().with_selected(if *editing { None } else { Some(*selected) });
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().fg(Color::Black).bg(ink()))
                    .block(panel("Folders and ROMs · Backspace: parent")),
                rows[1],
                &mut selection,
            );
            frame.render_widget(
                Paragraph::new(browser.note.clone()).wrap(Wrap { trim: false }),
                rows[2],
            );
        }
        Modal::Hash { expected } => {
            let area = center(frame, "File checksums · Esc: close · Ctrl+E: export");
            let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(area);
            frame.render_widget(
                Paragraph::new(state.app.hashes()).wrap(Wrap { trim: false }),
                rows[0],
            );
            frame.render_widget(
                Paragraph::new(expected.as_str()).block(panel(
                    "Paste expected CRC32 / SHA-1 / SHA-256 · Enter: compare",
                )),
                rows[1],
            );
        }
        Modal::Library { path } => {
            let area = center(frame, "Library folder · Esc: cancel");
            frame.render_widget(Paragraph::new(format!("Choose an existing absolute folder path.\n\n{path}\n\nCtrl+U clears. Enter saves the preference. Existing backups stay in their current folder.")).wrap(Wrap { trim: false }), area);
        }
        Modal::Stop => {
            let area = center(frame, "Stop operation?");
            frame.render_widget(Paragraph::new("Completed backups and partial reads will be retained.\n\nIf erase or writing has begun, restore the retained source ROM before using the cartridge. Keep USB connected while the service closes safely.\n\nEsc: Continue operation                 S: Stop safely").wrap(Wrap { trim: false }), area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    #[test]
    fn all_slot_buttons_fit_the_minimum_terminal() {
        let root = tempfile::tempdir().unwrap();
        let mut tui = Tui::new(cartridge_app::App::new(Some(root.path().into()), true));
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|f| draw(f, &mut tui)).unwrap();
        let buffer = terminal.backend().buffer();
        let text = (0..32)
            .map(|y| {
                (0..120)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for label in [
            "1  NES (72-pin)",
            "2  Famicom (60-pin)",
            "3  Game Boy / Color",
            "4  Game Boy Advance",
            "7  Preferences",
            "?  Field guide",
        ] {
            assert!(text.contains(label), "Missing or clipped label: {label}");
        }
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/slots-tui.txt");
        std::fs::write(out, text).unwrap();
    }
    #[test]
    fn sizes_and_pages_render_without_losing_resize_guard() {
        let root = tempfile::tempdir().unwrap();
        let mut tui = Tui::new(cartridge_app::App::new(Some(root.path().into()), true));
        for (w, h) in [(1, 1), (80, 24), (119, 32), (120, 31), (120, 32), (160, 50)] {
            for page in [
                Page::Workspace,
                Page::History,
                Page::Settings,
                Page::Support,
            ] {
                tui.app.page = page;
                let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| draw(f, &mut tui)).unwrap();
                if w < 120 || h < 32 {
                    assert!(tui.hits.is_empty());
                } else {
                    assert!(!tui.hits.is_empty());
                }
            }
        }
    }
    #[test]
    fn file_text_and_hash_dialogs_fit_minimum_terminal() {
        let root = tempfile::tempdir().unwrap();
        let mut tui = Tui::new(cartridge_app::App::new(Some(root.path().into()), true));
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        for modal in [
            Modal::Hash {
                expected: "a".repeat(64),
            },
            Modal::Text {
                title: "Details".into(),
                content: "A long report\n".repeat(100),
                scroll: 99,
            },
            Modal::Stop,
            Modal::Files {
                browser: cartridge_app::files::Browser::new(root.path()).unwrap(),
                selected: 0,
                editing: true,
            },
        ] {
            tui.modal = Some(modal);
            terminal.draw(|f| draw(f, &mut tui)).unwrap();
        }
    }
}
