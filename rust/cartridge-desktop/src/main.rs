#[cfg(test)]
mod tests;
mod view;
use cartridge_app::{
    files::Browser, worker, App, Error, Page, Platform, Profile, Result, DESKTOP_MIN, VERSION,
};
use iced::{
    event,
    keyboard::{self, key::Named, Key},
    window, Element, Event, Size, Subscription, Task, Theme,
};
use std::{path::PathBuf, process::Command, time::Duration};

#[derive(Clone, Debug)]
enum Modal {
    Files(Browser),
    Text { title: String, content: String },
    Hashes { expected: String },
    Stop,
    Save { path: String },
}
#[derive(Clone, Debug)]
enum Message {
    Tick,
    Event(Event),
    Action(&'static str),
    Platform(Platform),
    Profile(Profile),
    Reader(cartridge_app::ReaderKind),
    Page(Page),
    Load,
    FilePath(String),
    FileOpen(PathBuf),
    FileSubmit,
    FileUp,
    CloseModal,
    Hashes,
    Expected(String),
    Compare,
    Copy(String),
    ExportText(String),
    Activity,
    ErrorDetails,
    Report(String),
    ReviewWord(String),
    Approve,
    Stop,
    ConfirmStop,
    Double(bool),
    Strict(bool),
    GbaSize(cartridge_app::GbaSize),
    Artwork(bool),
    Light(bool),
    Library(String),
    SaveSettings,
    FetchArtwork,
    SaveRom,
    SavePath(String),
    SaveCopy,
}
struct Desktop {
    app: App,
    modal: Option<Modal>,
    size: Size,
    library_path: String,
}
impl Desktop {
    fn new(options: &Options) -> Self {
        let mut app = App::new(options.library.clone(), options.no_device);
        if let Some(path) = &options.rom {
            if let Err(e) = app.load(path) {
                app.fail(e);
            }
        } else if !app.no_device {
            if let Err(e) = app.begin("doctor") {
                app.fail(e);
            }
        }
        let library_path = app.settings.data_directory.display().to_string();
        Self {
            app,
            modal: None,
            size: options.size,
            library_path,
        }
    }
    fn small(&self) -> bool {
        self.size.width < DESKTOP_MIN.0 as f32 || self.size.height < DESKTOP_MIN.1 as f32
    }
    fn handle(&mut self, result: Result<()>) {
        if let Err(e) = result {
            self.app.fail(e);
        }
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        // A resize hides all controls and dialogs. Hidden controls cannot dispatch actions.
        if self.small()
            && !matches!(
                message,
                Message::Event(_) | Message::Tick | Message::Stop | Message::ConfirmStop
            )
        {
            return Task::none();
        }
        if (self.modal.is_some() || self.app.review.is_some())
            && matches!(
                message,
                Message::Action(_)
                    | Message::Platform(_)
                    | Message::Profile(_)
                    | Message::Reader(_)
                    | Message::Page(_)
                    | Message::Load
                    | Message::Double(_)
                    | Message::GbaSize(_)
                    | Message::Strict(_)
                    | Message::Artwork(_)
                    | Message::Light(_)
                    | Message::Library(_)
                    | Message::SaveSettings
                    | Message::FetchArtwork
                    | Message::SaveRom
                    | Message::Activity
                    | Message::Hashes
                    | Message::Report(_)
            )
        {
            return Task::none();
        }
        match message {
            Message::Tick => {
                self.app.poll();
            }
            Message::Event(event) => match event {
                Event::Window(
                    window::Event::Opened { size, .. } | window::Event::Resized(size),
                ) => self.size = size,
                Event::Window(window::Event::CloseRequested) => {
                    if self.app.busy() {
                        self.modal = Some(Modal::Stop);
                    } else {
                        return iced::exit();
                    }
                }
                Event::Window(window::Event::FileDropped(path))
                    if !self.app.busy()
                        && self.app.review.is_none()
                        && self.modal.is_none()
                        && !self.small() =>
                {
                    self.modal = None;
                    let r = self.app.load(&path);
                    self.handle(r);
                }
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    if key == Key::Named(Named::Escape) {
                        self.modal = None;
                        self.app.review = None;
                    }
                    if self.small() {
                        return Task::none();
                    }
                    if key == Key::Named(Named::Tab) {
                        return if modifiers.shift() {
                            iced::widget::operation::focus_previous()
                        } else {
                            iced::widget::operation::focus_next()
                        };
                    }
                    if modifiers.command()
                        && key == Key::Character("o".into())
                        && self.app.review.is_none()
                    {
                        return self.update(Message::Load);
                    }
                    if key == Key::Named(Named::F5)
                        && self.modal.is_none()
                        && self.app.review.is_none()
                    {
                        return self.update(Message::Action("doctor"));
                    }
                }
                _ => {}
            },
            Message::Action(action) => {
                let r = self.app.begin(action);
                self.handle(r);
            }
            Message::Platform(platform) => {
                let r = self.app.set_platform(platform);
                self.handle(r);
            }
            Message::Reader(reader) => {
                let r = self.app.set_reader(reader);
                self.handle(r);
            }
            Message::Profile(profile) => {
                let r = self.app.set_profile(profile);
                self.handle(r);
            }
            Message::Page(page) => {
                if !self.app.busy() {
                    self.app.page = page;
                    if page == Page::History {
                        let r = self.app.refresh_history();
                        self.handle(r);
                    }
                }
            }
            Message::Load => {
                if !self.app.busy() && self.app.review.is_none() {
                    match Browser::new(&self.app.settings.tui_last_folder) {
                        Ok(browser) => self.modal = Some(Modal::Files(browser)),
                        Err(e) => self.app.fail(e),
                    }
                }
            }
            Message::FilePath(path) => {
                if let Some(Modal::Files(browser)) = &mut self.modal {
                    browser.path = path;
                }
            }
            Message::FileUp => {
                if let Some(Modal::Files(browser)) = &self.modal {
                    if let Some(parent) = browser.folder.parent() {
                        return self.update(Message::FileOpen(parent.into()));
                    }
                }
            }
            Message::FileSubmit => {
                if let Some(Modal::Files(browser)) = &self.modal {
                    return self.update(Message::FileOpen(browser.path.clone().into()));
                }
            }
            Message::FileOpen(path) => {
                if path.is_dir() {
                    if let Some(Modal::Files(browser)) = &mut self.modal {
                        if let Err(e) = browser.navigate(&path) {
                            browser.note = e.to_string();
                        }
                    }
                } else if !self.app.busy() {
                    self.modal = None;
                    self.app.page = Page::Workspace;
                    let r = self.app.load(&path);
                    self.handle(r);
                }
            }
            Message::CloseModal => {
                self.modal = None;
                self.app.review = None;
            }
            Message::Hashes => {
                self.modal = Some(Modal::Hashes {
                    expected: String::new(),
                })
            }
            Message::Expected(value) => {
                if let Some(Modal::Hashes { expected }) = &mut self.modal {
                    *expected = value;
                }
            }
            Message::Compare => {
                if let Some(Modal::Hashes { expected }) = self.modal.take() {
                    let r = self.app.checksum(&expected);
                    self.handle(r);
                }
            }
            Message::Copy(value) => return iced::clipboard::write(value),
            Message::ExportText(value) => {
                let r = self.app.export("details", &value).map(|_| ());
                self.handle(r);
                self.modal = None;
            }
            Message::Activity => {
                self.modal = Some(Modal::Text {
                    title: "Activity".into(),
                    content: self.app.log.iter().cloned().collect::<Vec<_>>().join("\n"),
                })
            }
            Message::Report(content) => {
                self.modal = Some(Modal::Text {
                    title: "Operation report".into(),
                    content,
                })
            }
            Message::ErrorDetails => {
                if let Some(e) = &self.app.error {
                    self.modal = Some(Modal::Text {
                        title: "Action needed".into(),
                        content: format!(
                            "{e}\n\n{}",
                            serde_json::to_string_pretty(&e.details).unwrap_or_default()
                        ),
                    });
                }
            }
            Message::ReviewWord(word) => {
                if let Some(review) = &mut self.app.review {
                    review.word = word;
                }
            }
            Message::Approve => {
                let r = self.app.approve();
                self.handle(r);
            }
            Message::Stop => {
                if self.app.busy() {
                    self.modal = Some(Modal::Stop);
                }
            }
            Message::ConfirmStop => {
                self.app.stop();
                self.modal = None;
            }
            Message::GbaSize(size) => {
                if !self.app.busy() && self.app.review.is_none() {
                    self.app.settings.gba_size = size;
                    let r = self.app.save_settings();
                    self.handle(r);
                }
            }
            Message::Double(value)
            | Message::Strict(value)
            | Message::Artwork(value)
            | Message::Light(value) => {
                if !self.app.busy() && self.app.review.is_none() {
                    match message {
                        Message::Double(_) => self.app.settings.double_read = value,
                        Message::Strict(_) => self.app.settings.strict_checksum = value,
                        Message::Artwork(_) => self.app.settings.download_artwork = value,
                        _ => self.app.settings.color_scheme = if value { 2 } else { 1 },
                    }
                    let r = self.app.save_settings();
                    self.handle(r);
                }
            }
            Message::Library(value) => self.library_path = value,
            Message::SaveSettings => {
                let path = PathBuf::from(&self.library_path);
                if !self.app.busy() {
                    if path.is_absolute() && path.is_dir() {
                        self.app.settings.data_directory = path;
                        let r = self.app.save_settings();
                        self.handle(r);
                    } else {
                        self.app.fail(Error::new(
                            "LIBRARY_UNAVAILABLE",
                            "Choose an existing absolute folder path.",
                            "Create or select a folder you own, then save the preference.",
                        ));
                    }
                }
            }
            Message::FetchArtwork => {
                let r = self.app.begin("game");
                self.handle(r);
            }
            Message::SaveRom => {
                if let Some(source) = &self.app.source {
                    self.modal = Some(Modal::Save {
                        path: self
                            .app
                            .settings
                            .data_directory
                            .join(source.path.file_name().unwrap_or_default())
                            .display()
                            .to_string(),
                    });
                }
            }
            Message::SavePath(value) => {
                if let Some(Modal::Save { path }) = &mut self.modal {
                    *path = value;
                }
            }
            Message::SaveCopy => {
                if let Some(Modal::Save { path }) = self.modal.take() {
                    let r = self.app.save_rom(&PathBuf::from(path));
                    self.handle(r);
                }
            }
        }
        Task::none()
    }
    fn subscription(&self) -> Subscription<Message> {
        let events = event::listen_with(|event, status, _| presentation_event(event, status));
        if self.app.busy() {
            Subscription::batch([
                events,
                iced::time::every(Duration::from_millis(80)).map(|_| Message::Tick),
            ])
        } else {
            events
        }
    }
    fn theme(&self) -> Theme {
        if self.app.settings.color_scheme == 2 {
            Theme::Light
        } else {
            Theme::custom(
                "Cartridge Night",
                iced::theme::Palette {
                    background: iced::Color::from_rgb8(17, 24, 31),
                    text: iced::Color::from_rgb8(229, 237, 240),
                    primary: iced::Color::from_rgb8(104, 210, 181),
                    success: iced::Color::from_rgb8(104, 210, 181),
                    warning: iced::Color::from_rgb8(234, 189, 109),
                    danger: iced::Color::from_rgb8(245, 129, 126),
                },
            )
        }
    }
    fn view(&self) -> Element<'_, Message> {
        view::view(self)
    }
}
fn presentation_event(event: Event, status: event::Status) -> Option<Message> {
    match event {
        Event::Window(
            window::Event::Opened { .. }
            | window::Event::Resized(_)
            | window::Event::CloseRequested
            | window::Event::FileDropped(_),
        ) => Some(Message::Event(event)),
        Event::Keyboard(_) if status == event::Status::Ignored => Some(Message::Event(event)),
        // RedrawRequested must never create an application message: doing so
        // schedules another redraw and turns an idle window into a render loop.
        _ => None,
    }
}
#[derive(Clone)]
struct Options {
    library: Option<PathBuf>,
    rom: Option<PathBuf>,
    no_device: bool,
    size: Size,
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut options = Options {
        library: None,
        rom: None,
        no_device: false,
        size: Size::new(1280., 860.),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("Cartridge Studio {VERSION} · Rust desktop");
                return Ok(());
            }
            "--help" | "-h" => {
                println!("Cartridge Studio\n\ncartridge-studio [ROM] [--library FOLDER] [--no-device]\ncartridge-studio --tui [OPTIONS]\ncartridge-studio --cli [COMMAND]\n\nNative Rust desktop and terminal; no Python, GTK or browser runtime.\nA window of at least 1000 × 700 is required for the desktop workspace.");
                return Ok(());
            }
            "--tui" | "--cli" | "--worker" => {
                let name = match arg.as_str() {
                    "--tui" => "cartridge-tui",
                    "--worker" => "cartridge-worker",
                    _ => "cartridge",
                };
                let mut command = Command::new(worker::binary(name)?);
                if let Some(root) = options.library {
                    command.arg("--library").arg(root);
                }
                if options.no_device && name == "cartridge-tui" {
                    command.arg("--no-device");
                }
                command.args(args);
                use std::os::unix::process::CommandExt;
                return Err(command.exec().into());
            }
            "--library" => {
                options.library = Some(args.next().ok_or("--library needs a folder path")?.into())
            }
            "--no-device" => options.no_device = true,
            "--width" => {
                options.size.width = args.next().ok_or("--width needs a number")?.parse()?
            }
            "--height" => {
                options.size.height = args.next().ok_or("--height needs a number")?.parse()?
            }
            _ if !arg.starts_with('-') && options.rom.is_none() => options.rom = Some(arg.into()),
            _ => return Err(format!("Unknown option: {arg}. Run --help for usage.").into()),
        }
    }
    let size = options.size;
    #[cfg(target_os = "linux")]
    let platform_specific = window::settings::PlatformSpecific {
        application_id: "io.github.borjaburgos.CartridgeStudio".into(),
        ..Default::default()
    };
    #[cfg(not(target_os = "linux"))]
    let platform_specific = Default::default();
    iced::application(
        move || Desktop::new(&options),
        Desktop::update,
        Desktop::view,
    )
    .title("Cartridge Studio")
    .theme(Desktop::theme)
    .subscription(Desktop::subscription)
    .default_font(iced::Font::with_name("Fira Sans"))
    .window(window::Settings {
        size,
        platform_specific,
        exit_on_close_request: false,
        ..Default::default()
    })
    .run()?;
    Ok(())
}
