mod ui;
use cartridge_app::{
    files::{self, Browser},
    App, Error, Page, Platform, Result, TERMINAL_MIN, VERSION,
};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseEventKind,
    },
    execute,
};
use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    time::Duration,
};

#[derive(Clone, Debug)]
enum Modal {
    Files {
        browser: Browser,
        selected: usize,
        editing: bool,
    },
    Text {
        title: String,
        content: String,
        scroll: u16,
    },
    Hash {
        expected: String,
    },
    Library {
        path: String,
    },
    Stop,
}
#[derive(Clone, Debug)]
enum Command {
    Action(&'static str),
    Platform(Platform),
    Page(Page),
    Load,
    Profile,
    Reader,
    Double,
    Strict,
    GbaSize,
    Artwork,
    Theme,
    Hashes,
    Activity,
    Help,
    Stop,
    Quit,
    Error,
}
struct Tui {
    app: App,
    modal: Option<Modal>,
    focus: usize,
    history_selected: usize,
    support_scroll: u16,
    hits: Vec<(Rect, Command)>,
    quit: bool,
    size: (u16, u16),
}
impl Tui {
    fn new(app: App) -> Self {
        Self {
            app,
            modal: None,
            focus: 0,
            history_selected: 0,
            support_scroll: 0,
            hits: vec![],
            quit: false,
            size: TERMINAL_MIN,
        }
    }
    fn handle(&mut self, result: Result<()>) {
        if let Err(e) = result {
            self.app.fail(e);
        }
    }
    fn command(&mut self, command: Command) {
        if self.app.review.is_some() || self.modal.is_some() {
            return;
        }
        let result = match command {
            Command::Action(action) => self.app.begin(action),
            Command::Platform(p) => self.app.set_platform(p),
            Command::Page(page) => {
                self.app.page = page;
                if page == Page::History {
                    self.app.refresh_history()
                } else {
                    Ok(())
                }
            }
            Command::Load => {
                if self.app.busy() {
                    return;
                }
                match Browser::new(&self.app.settings.tui_last_folder) {
                    Ok(browser) => {
                        self.modal = Some(Modal::Files {
                            browser,
                            selected: 0,
                            editing: true,
                        })
                    }
                    Err(e) => self.app.fail(e),
                };
                Ok(())
            }
            Command::Reader => {
                let choices = cartridge_app::ReaderKind::ALL;
                let index = choices
                    .iter()
                    .position(|k| *k == self.app.settings.reader)
                    .unwrap_or(0);
                self.app.set_reader(choices[(index + 1) % choices.len()])
            }
            Command::GbaSize => {
                if self.app.busy() {
                    return;
                }
                let all = cartridge_app::GbaSize::ALL;
                let n = all
                    .iter()
                    .position(|&s| s == self.app.settings.gba_size)
                    .unwrap_or(0);
                self.app.settings.gba_size = all[(n + 1) % all.len()];
                self.app.save_settings()
            }
            Command::Profile => {
                let profiles = self.app.platform.profiles();
                let index = profiles
                    .iter()
                    .position(|p| *p == self.app.profile)
                    .unwrap_or(0);
                self.app.set_profile(profiles[(index + 1) % profiles.len()])
            }
            Command::Double | Command::Strict | Command::Artwork | Command::Theme => {
                if self.app.busy() {
                    return;
                }
                match command {
                    Command::Double => self.app.settings.double_read ^= true,
                    Command::Strict => self.app.settings.strict_checksum ^= true,
                    Command::Artwork => self.app.settings.download_artwork ^= true,
                    _ => {
                        self.app.settings.color_scheme = if self.app.settings.color_scheme == 2 {
                            1
                        } else {
                            2
                        }
                    }
                }
                self.app.save_settings()
            }
            Command::Hashes => {
                self.modal = Some(Modal::Hash {
                    expected: String::new(),
                });
                Ok(())
            }
            Command::Activity => {
                self.modal = Some(Modal::Text {
                    title: "Activity · E to export".into(),
                    content: self.app.log.iter().cloned().collect::<Vec<_>>().join("\n"),
                    scroll: 0,
                });
                Ok(())
            }
            Command::Error => {
                if let Some(e) = &self.app.error {
                    self.modal = Some(Modal::Text {
                        title: "Action needed · E to export".into(),
                        content: format!(
                            "{e}\n\n{}",
                            serde_json::to_string_pretty(&e.details).unwrap_or_default()
                        ),
                        scroll: 0,
                    });
                }
                Ok(())
            }
            Command::Help => {
                self.modal = Some(Modal::Text {
                    title: "Field guide".into(),
                    content: HELP.into(),
                    scroll: 0,
                });
                Ok(())
            }
            Command::Stop => {
                if self.app.busy() {
                    self.modal = Some(Modal::Stop);
                }
                Ok(())
            }
            Command::Quit => {
                if self.app.busy() {
                    self.modal = Some(Modal::Stop);
                } else {
                    self.quit = true;
                }
                Ok(())
            }
        };
        self.handle(result);
    }
    fn key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if self.size.0 < TERMINAL_MIN.0 || self.size.1 < TERMINAL_MIN.1 {
            if matches!(key.code, KeyCode::Char('q')) && !self.app.busy() {
                self.quit = true;
            }
            if matches!(key.code, KeyCode::Char('x')) && self.app.busy() {
                self.app.stop();
            }
            return;
        }
        if let Some(review) = self.app.review.as_mut() {
            match key.code {
                KeyCode::Esc => self.app.review = None,
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL) && review.word.len() < 8 =>
                {
                    review.word.push(c)
                }
                KeyCode::Backspace => {
                    review.word.pop();
                }
                KeyCode::Enter if review.ready() => {
                    let r = self.app.approve();
                    self.handle(r);
                }
                _ => {}
            }
            return;
        }
        if let Some(modal) = self.modal.take() {
            self.modal = self.modal_key(modal, key);
            return;
        }
        if self.app.page == Page::Support {
            match key.code {
                KeyCode::Down => {
                    self.support_scroll = self.support_scroll.saturating_add(1);
                    return;
                }
                KeyCode::Up => {
                    self.support_scroll = self.support_scroll.saturating_sub(1);
                    return;
                }
                KeyCode::PageDown => {
                    self.support_scroll = self.support_scroll.saturating_add(10);
                    return;
                }
                KeyCode::PageUp => {
                    self.support_scroll = self.support_scroll.saturating_sub(10);
                    return;
                }
                KeyCode::Home => {
                    self.support_scroll = 0;
                    return;
                }
                _ => {}
            }
        }
        if self.app.page == Page::History {
            match key.code {
                KeyCode::Up => {
                    self.history_selected = self.history_selected.saturating_sub(1);
                    return;
                }
                KeyCode::Down => {
                    self.history_selected =
                        (self.history_selected + 1).min(self.app.history.len().saturating_sub(1));
                    return;
                }
                KeyCode::Enter => {
                    if let Some(h) = self.app.history.get(self.history_selected) {
                        self.modal = Some(Modal::Text {
                            title: "Operation report · E to export".into(),
                            content: serde_json::to_string_pretty(&h.report).unwrap_or_default(),
                            scroll: 0,
                        });
                    }
                    return;
                }
                KeyCode::Char('o') if !self.app.busy() => {
                    if let Some(path) = self
                        .app
                        .history
                        .get(self.history_selected)
                        .and_then(|h| h.report["output"].as_str())
                        .map(PathBuf::from)
                    {
                        let r = self.app.load(&path);
                        self.app.page = Page::Workspace;
                        self.handle(r);
                    }
                    return;
                }
                _ => {}
            }
        }
        if self.app.page == Page::Settings && key.code == KeyCode::Char('f') && !self.app.busy() {
            self.modal = Some(Modal::Library {
                path: self.app.settings.data_directory.display().to_string(),
            });
            return;
        }
        match key.code {
            KeyCode::Tab => {
                self.focus = (self.focus + 1) % self.hits.len().max(1);
                return;
            }
            KeyCode::BackTab => {
                self.focus =
                    (self.focus + self.hits.len().saturating_sub(1)) % self.hits.len().max(1);
                return;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some((_, cmd)) = self.hits.get(self.focus) {
                    self.command(cmd.clone());
                }
                return;
            }
            _ => {}
        }
        let command = match key.code {
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Command::Load)
            }
            KeyCode::Char('q') | KeyCode::Char('c')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                Some(Command::Quit)
            }
            KeyCode::Char('q') => Some(Command::Quit),
            KeyCode::Char('1') => Some(Command::Platform(Platform::Nes)),
            KeyCode::Char('2') => Some(Command::Platform(Platform::Famicom)),
            KeyCode::Char('3') => Some(Command::Platform(Platform::GameBoy)),
            KeyCode::Char('4') => Some(Command::Platform(Platform::Gba)),
            KeyCode::Char('5') => Some(Command::Page(Page::History)),
            KeyCode::Char('6') => Some(Command::Page(Page::Support)),
            KeyCode::Char('7') => Some(Command::Page(Page::Settings)),
            KeyCode::Char('p') => Some(Command::Action("probe")),
            KeyCode::Char('u') => Some(Command::Reader),
            KeyCode::F(5) => Some(Command::Action("doctor")),
            KeyCode::Char('r') => Some(Command::Action("read")),
            KeyCode::Char('b') => Some(Command::Action("backup")),
            KeyCode::Char('v') => Some(Command::Action("verify")),
            KeyCode::Char('c') => Some(Command::Action("check")),
            KeyCode::Char('w') => Some(Command::Action("write")),
            KeyCode::Char('W') => Some(Command::Action("wipe")),
            KeyCode::Char('h') => Some(Command::Hashes),
            KeyCode::Char('l') => Some(Command::Activity),
            KeyCode::Char('m') => Some(Command::Profile),
            KeyCode::Char('g') => Some(Command::GbaSize),
            KeyCode::Char('d') => Some(Command::Double),
            KeyCode::Char('s') => Some(Command::Strict),
            KeyCode::Char('a') => Some(Command::Artwork),
            KeyCode::Char('t') => Some(Command::Theme),
            KeyCode::Char('x') => Some(Command::Stop),
            KeyCode::Char('e') => Some(Command::Error),
            KeyCode::Char('?') | KeyCode::F(1) => Some(Command::Help),
            _ => None,
        };
        if let Some(command) = command {
            self.command(command);
        }
    }
    fn modal_key(&mut self, mut modal: Modal, key: KeyEvent) -> Option<Modal> {
        if key.code == KeyCode::Esc {
            return None;
        }
        match &mut modal {
            Modal::Files {
                browser,
                selected,
                editing,
            } => match key.code {
                KeyCode::Tab | KeyCode::BackTab => *editing ^= true,
                KeyCode::Char(c) if *editing && !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if browser.path.len() < 4096 {
                        browser.path.push(c);
                    }
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) && *editing => {
                    browser.path.clear()
                }
                KeyCode::Backspace if *editing => {
                    browser.path.pop();
                }
                KeyCode::Backspace => {
                    if let Some(parent) = browser.folder.parent().map(PathBuf::from) {
                        let r = browser.navigate(&parent);
                        self.handle(r);
                        *selected = 0;
                    }
                }
                KeyCode::Up if !*editing => *selected = selected.saturating_sub(1),
                KeyCode::Down if !*editing => {
                    *selected = (*selected + 1).min(browser.entries.len().saturating_sub(1))
                }
                KeyCode::Enter => {
                    let path = if *editing {
                        PathBuf::from(&browser.path)
                    } else if let Some(e) = browser.entries.get(*selected) {
                        e.path.clone()
                    } else {
                        return Some(modal);
                    };
                    if path.is_dir() {
                        let r = browser.navigate(&path);
                        self.handle(r);
                        *selected = 0;
                        *editing = false;
                    } else {
                        let r = self.app.load(&path);
                        self.handle(r);
                        return None;
                    }
                }
                _ => {}
            },
            Modal::Text {
                title,
                content,
                scroll,
            } => match key.code {
                KeyCode::Down => *scroll = scroll.saturating_add(1),
                KeyCode::Up => *scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => *scroll = scroll.saturating_add(15),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(15),
                KeyCode::Home => *scroll = 0,
                KeyCode::Char('e') => {
                    let r = self.app.export("details", content).map(|_| ());
                    self.handle(r);
                    *title = "Export saved in library/exports".into();
                }
                _ => {}
            },
            Modal::Hash { expected } => match key.code {
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL) && expected.len() < 128 =>
                {
                    expected.push(c)
                }
                KeyCode::Backspace => {
                    expected.pop();
                }
                KeyCode::Enter => {
                    let r = self.app.checksum(expected);
                    self.handle(r);
                    return None;
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let r = self.app.export("checksums", &self.app.hashes()).map(|_| ());
                    self.handle(r);
                }
                _ => {}
            },
            Modal::Library { path } => match key.code {
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL) && path.len() < 4096 =>
                {
                    path.push(c)
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => path.clear(),
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Enter => {
                    let p = PathBuf::from(&path);
                    if p.is_absolute() && p.is_dir() {
                        self.app.settings.data_directory = p;
                        let r = self.app.save_settings();
                        self.handle(r);
                        return None;
                    }
                    self.app.fail(Error::new(
                        "LIBRARY_UNAVAILABLE",
                        "Choose an existing absolute folder path.",
                        "Create or select a folder you own, then retry.",
                    ));
                }
                _ => {}
            },
            Modal::Stop => {
                if key.code == KeyCode::Char('s') {
                    self.app.stop();
                    return None;
                }
            }
        }
        Some(modal)
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut library = None;
    let mut rom = None;
    let mut no_device = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("Cartridge Studio {VERSION} · Rust terminal");
                return Ok(());
            }
            "--help" | "-h" => {
                println!("Cartridge Studio — Rust terminal\n\ncartridge-tui [--library FOLDER] [--no-device] [ROM]\n\n{HELP}");
                return Ok(());
            }
            "--library" => {
                library =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        Error::check("--library needs a folder path.")
                    })?))
            }
            "--no-device" => no_device = true,
            _ if !arg.starts_with('-') && rom.is_none() => rom = Some(PathBuf::from(arg)),
            _ => {
                return Err(Error::new(
                    "UNKNOWN_OPTION",
                    format!("Unknown option: {arg}"),
                    "Run cartridge-tui --help for available options.",
                ))
            }
        }
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::new(
            "TERMINAL_REQUIRED",
            "The terminal interface needs an interactive terminal.",
            "Open a terminal and run cartridge tui. For scripts, use cartridge --json instead.",
        ));
    }
    let mut state = Tui::new(App::new(library, no_device));
    if let Some(path) = rom {
        let r = state.app.load(&path);
        state.handle(r);
    } else if !no_device {
        let r = state.app.begin("doctor");
        state.handle(r);
    }
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnableMouseCapture,
        event::EnableBracketedPaste
    )?;
    event_loop(&mut terminal, &mut state)
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            event::DisableBracketedPaste
        );
        ratatui::restore();
    }
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut Tui,
) -> Result<()> {
    let mut dirty = true;
    while !state.quit {
        dirty |= state.app.poll();
        if dirty {
            terminal.draw(|frame| {
                state.size = (frame.area().width, frame.area().height);
                ui::draw(frame, state);
            })?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(if state.app.busy() {
            50
        } else {
            1000
        }))? {
            match event::read()? {
                Event::Key(key) => state.key(key),
                Event::Mouse(mouse)
                    if mouse.kind == MouseEventKind::Down(event::MouseButton::Left) =>
                {
                    if let Some((index, (_, command))) = state
                        .hits
                        .iter()
                        .enumerate()
                        .find(|(_, (r, _))| r.contains((mouse.column, mouse.row).into()))
                    {
                        let command = command.clone();
                        state.focus = index;
                        state.command(command);
                    }
                }
                Event::Paste(text)
                    if state.size.0 >= TERMINAL_MIN.0 && state.size.1 >= TERMINAL_MIN.1 =>
                {
                    let text = files::plain(&text).replace(['\n', '\t'], "");
                    if let Some(review) = state.app.review.as_mut() {
                        review.word = text.chars().take(8).collect();
                    } else {
                        match state.modal.as_mut() {
                            Some(Modal::Files {
                                browser,
                                editing: true,
                                ..
                            }) => browser
                                .path
                                .push_str(&text.chars().take(4096).collect::<String>()),
                            Some(Modal::Hash { expected }) => {
                                expected.push_str(&text.chars().take(128).collect::<String>())
                            }
                            Some(Modal::Library { path }) => {
                                path.push_str(&text.chars().take(4096).collect::<String>())
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            dirty = true;
        }
    }
    Ok(())
}

const HELP: &str = "1 / 2 / 3 / 4 NES / Famicom / Game Boy & Color / Game Boy Advance\n5 / 6 / 7 History / Supported boards / Preferences\nCtrl+O    Browse or paste a ROM path\nU / F5    Choose reader / Check USB\nP / M     Detect cartridge / Change board profile\nR / B / V Read / Backup / Verify cartridge\nC         Check ROM compatibility offline\nW         Review write    Shift+W  Review wipe\nH / L / E Checksums / Activity / Error details\nD / S / A Double read / Strict GB checks / Artwork\nG         Cycle GBA ROM size (shown in Preferences)\nT         Toggle light/dark theme\nX         Review stopping the current operation\nQ         Quit when idle\nTab       Focus next button    Enter  Activate\nEscape    Cancel dialog\n\nFILE BROWSER\nTab switches path/list. Up/Down selects an entry. Enter opens. Backspace moves to parent in list mode. Ctrl+U clears the path. Bracketed paste is supported.\n\nHISTORY\nUp/Down selects a backup. Enter opens its report; O loads its saved ROM. Reports are retained for incomplete operations too.\n\nWRITE / WIPE\nType the full uppercase confirmation word. Reader, source SHA-256, platform and board are pinned to the review. Mandatory backups and write verification cannot be disabled by read options.\n\nWINDOW SIZE\nMinimum 120 columns × 32 rows. A smaller terminal shows a resize notice while active operations continue. X can still stop safely.\n\nBackups and settings use the same library as the desktop. Read saves a playable-format ROM when its cartridge format is supported; save RAM and RTC are not included.\n\nNeither physical button is normally needed. Disconnect USB before changing cartridges and connect only one cartridge.";

#[cfg(test)]
mod input_tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn reader_shortcut_changes_settings_and_invalidates_connection() {
        let root = tempfile::tempdir().unwrap();
        let mut state = Tui::new(App::new(Some(root.path().into()), false));
        state.app.connected = true;
        state.key(key(KeyCode::Char('u')));
        assert!(!state.app.connected);
        assert_eq!(
            state.app.settings.reader,
            cartridge_app::ReaderKind::Inlretro
        );
        state.key(key(KeyCode::Char('u')));
        assert_eq!(
            state.app.settings.reader,
            cartridge_app::ReaderKind::Gbxcart
        );
        state.app.connected = true;
        assert!(state.app.can("read"));
        state.key(key(KeyCode::Char('W')));
        assert!(state.app.review.is_none());
        assert_eq!(
            state.app.error.as_ref().unwrap().code,
            "READER_OPERATION_UNSUPPORTED"
        );
    }
    #[test]
    fn resize_blocks_hidden_commands_and_keeps_the_dialog() {
        let root = tempfile::tempdir().unwrap();
        let mut state = Tui::new(App::new(Some(root.path().into()), true));
        state.command(Command::Load);
        assert!(state.modal.is_some());
        state.size = (80, 24);
        state.key(key(KeyCode::Esc));
        assert!(state.modal.is_some());
        state.key(key(KeyCode::Char('1')));
        assert_eq!(state.app.platform, Platform::GameBoy);
        state.size = TERMINAL_MIN;
        state.key(key(KeyCode::Esc));
        assert!(state.modal.is_none());
    }
    #[test]
    fn typing_in_file_path_does_not_dispatch_global_actions() {
        let root = tempfile::tempdir().unwrap();
        let mut state = Tui::new(App::new(Some(root.path().into()), true));
        state.command(Command::Load);
        for c in "write.gb".chars() {
            state.key(key(KeyCode::Char(c)));
        }
        assert!(!state.app.busy());
        assert!(state.app.review.is_none());
        assert!(matches!(state.modal, Some(Modal::Files { .. })));
    }
    #[test]
    fn slot_shortcuts_select_the_three_physical_connectors() {
        let root = tempfile::tempdir().unwrap();
        let mut state = Tui::new(App::new(Some(root.path().into()), true));
        state.size = TERMINAL_MIN;
        for (key_value, slot) in [
            ('1', Platform::Nes),
            ('2', Platform::Famicom),
            ('3', Platform::GameBoy),
            ('4', Platform::Gba),
        ] {
            state.key(key(KeyCode::Char(key_value)));
            assert_eq!(state.app.platform, slot);
        }
    }
    #[test]
    fn wipe_confirmation_rejects_wrong_word_and_escape_cancels() {
        let root = tempfile::tempdir().unwrap();
        let mut state = Tui::new(App::new(Some(root.path().into()), false));
        state.app.connected = true;
        state
            .app
            .set_profile(state.app.platform.profiles()[1])
            .unwrap();
        state.command(Command::Action("wipe"));
        assert!(state.app.review.is_some());
        for c in "WRITE".chars() {
            state.key(key(KeyCode::Char(c)));
        }
        state.key(key(KeyCode::Enter));
        assert!(!state.app.busy());
        state.key(key(KeyCode::Esc));
        assert!(state.app.review.is_none());
    }
    #[test]
    fn support_can_scroll_to_the_qualification_limits() {
        let root = tempfile::tempdir().unwrap();
        let mut state = Tui::new(App::new(Some(root.path().into()), true));
        state.command(Command::Page(Page::Support));
        state.key(key(KeyCode::PageDown));
        assert_eq!(state.support_scroll, 10);
        state.key(key(KeyCode::Home));
        assert_eq!(state.support_scroll, 0);
    }
}
