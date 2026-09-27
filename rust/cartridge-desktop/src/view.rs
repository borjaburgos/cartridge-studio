use super::{Desktop, Message, Modal};
use cartridge_app::{value_text, Page, Platform, DESKTOP_MIN, SUPPORT, VERSION};
use iced::{
    alignment::Vertical,
    widget::{
        self, button, checkbox, column, container, image, pick_list, row, scrollable, space, stack,
        svg, text, text_input, Column,
    },
    Border, Color, Element,
    Length::{Fill, FillPortion},
    Theme,
};

fn secondary(theme: &Theme) -> widget::text::Style {
    widget::text::Style {
        color: Some(if theme.extended_palette().is_dark {
            Color::from_rgb8(132, 154, 166)
        } else {
            Color::from_rgb8(64, 84, 96)
        }),
    }
}
fn accent_text(theme: &Theme) -> widget::text::Style {
    widget::text::Style {
        color: Some(if theme.extended_palette().is_dark {
            Color::from_rgb8(104, 210, 181)
        } else {
            Color::from_rgb8(25, 104, 78)
        }),
    }
}
fn error_text(theme: &Theme) -> widget::text::Style {
    widget::text::Style {
        color: Some(if theme.extended_palette().is_dark {
            Color::from_rgb8(237, 139, 126)
        } else {
            Color::from_rgb8(155, 50, 39)
        }),
    }
}
fn heading(label: &str) -> widget::Text<'_> {
    text(label).size(12).style(secondary)
}
fn card<'a>(content: impl Into<Element<'a, Message>>) -> widget::Container<'a, Message> {
    container(content)
        .padding(18)
        .width(Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.background.weak.color.into()),
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.,
                    radius: 12.into(),
                },
                ..Default::default()
            }
        })
}
fn control<'a>(label: &'a str, message: Message, enabled: bool) -> widget::Button<'a, Message> {
    button(text(label).size(14))
        .padding([10, 14])
        .on_press_maybe(enabled.then_some(message))
        .style(button::secondary)
}
fn primary<'a>(label: &'a str, message: Message, enabled: bool) -> widget::Button<'a, Message> {
    button(text(label).size(14))
        .padding([10, 16])
        .on_press_maybe(enabled.then_some(message))
}
fn platform_logo(platform: Platform, color: bool) -> Element<'static, Message> {
    let bytes: &'static [u8] = match platform {
        Platform::Nes => include_bytes!("../../../desktop/assets/nes.svg"),
        Platform::Famicom => include_bytes!("../../../desktop/assets/famicom.svg"),
        Platform::GameBoy if color => include_bytes!("../../../desktop/assets/gameboy-color.svg"),
        Platform::GameBoy => include_bytes!("../../../desktop/assets/gameboy.svg"),
        Platform::Gba => include_bytes!("../../../desktop/assets/gameboy-advance.svg"),
    };
    container(svg(svg::Handle::from_memory(bytes)).width(176).height(44))
        .padding([10, 14])
        .style(|_| container::Style {
            background: Some(Color::from_rgb8(249, 248, 243).into()),
            border: Border {
                radius: 8.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

pub fn view(state: &Desktop) -> Element<'_, Message> {
    let app = &state.app;
    if state.small() {
        let mut content = column![
            text("Window too small").size(28),
            text(format!(
                "Resize to at least {} × {} to use the workspace.\nCurrent size: {:.0} × {:.0}",
                DESKTOP_MIN.0, DESKTOP_MIN.1, state.size.width, state.size.height
            ))
            .size(16),
            text(&app.status).size(14)
        ]
        .spacing(18)
        .max_width(600);
        if app.busy() {
            content = content
                .push(text("Your operation continues. Keep USB connected."))
                .push(control(
                    "Stop safely",
                    Message::ConfirmStop,
                    !app.stopping(),
                ));
        }
        return container(content)
            .padding(24)
            .center_x(Fill)
            .center_y(Fill)
            .into();
    }
    let sidebar = column![
        row![
            svg(svg::Handle::from_memory(
                include_bytes!("../../../desktop/assets/studio.svg").as_slice()
            ))
            .width(34)
            .height(34),
            column![
                text("Cartridge").size(22),
                text("STUDIO").size(10).style(secondary)
            ]
        ]
        .spacing(12)
        .align_y(Vertical::Center),
        space().height(12),
        heading("CARTRIDGE SLOTS"),
        nav(
            "NES",
            Message::Platform(Platform::Nes),
            app.page == Page::Workspace && app.platform == Platform::Nes,
            !app.busy()
        ),
        nav(
            "Famicom",
            Message::Platform(Platform::Famicom),
            app.page == Page::Workspace && app.platform == Platform::Famicom,
            !app.busy()
        ),
        nav(
            "Game Boy / Color",
            Message::Platform(Platform::GameBoy),
            app.page == Page::Workspace && app.platform == Platform::GameBoy,
            !app.busy()
        ),
        nav(
            "Game Boy Advance",
            Message::Platform(Platform::Gba),
            app.page == Page::Workspace && app.platform == Platform::Gba,
            !app.busy()
        ),
        space().height(12),
        heading("LIBRARY"),
        nav(
            "Backup history",
            Message::Page(Page::History),
            app.page == Page::History,
            !app.busy()
        ),
        nav(
            "Supported boards",
            Message::Page(Page::Support),
            app.page == Page::Support,
            !app.busy()
        ),
        nav(
            "Preferences",
            Message::Page(Page::Settings),
            app.page == Page::Settings,
            !app.busy()
        ),
        space().height(Fill),
        heading("READER"),
        pick_list(
            if app.busy() {
                vec![]
            } else {
                cartridge_app::ReaderKind::ALL.to_vec()
            },
            Some(app.settings.reader),
            Message::Reader
        )
        .text_size(13)
        .width(Fill),
        text(if app.connected {
            "● Reader connected"
        } else {
            "○ Reader not ready"
        })
        .size(13)
        .style(move |theme| if app.connected {
            accent_text(theme)
        } else {
            secondary(theme)
        }),
        text(&app.reader).size(12).style(secondary),
        control("Check USB", Message::Action("doctor"), app.can("doctor")).width(Fill),
        text(format!("VERSION {VERSION} · RUST"))
            .size(10)
            .style(secondary),
    ]
    .spacing(7)
    .padding(18)
    .width(210)
    .height(Fill);
    let header = row![
        column![
            text(match app.page {
                Page::Workspace => "Cartridge workspace",
                Page::History => "Backup history",
                Page::Support => "Supported cartridges",
                Page::Settings => "Preferences",
            })
            .size(20),
            text("Preserve. Restore. Play.").size(12).style(secondary)
        ],
        space().width(Fill),
        control("Activity", Message::Activity, true),
        primary("Load ROM", Message::Load, !app.busy())
    ]
    .spacing(12)
    .align_y(Vertical::Center)
    .padding([18, 24]);
    let body = match app.page {
        Page::Workspace => workspace(state),
        Page::History => history(state),
        Page::Support => scrollable(
            column![
                text("Know what your cartridge supports").size(28),
                card(text(SUPPORT).size(16))
            ]
            .spacing(20)
            .padding(24),
        )
        .height(Fill)
        .into(),
        Page::Settings => settings(state),
    };
    let base = row![
        container(sidebar).style(|theme: &Theme| container::Style {
            background: Some(theme.extended_palette().background.weak.color.into()),
            ..Default::default()
        }),
        column![header, widget::rule::horizontal(1), body].width(Fill)
    ]
    .height(Fill);
    if app.review.is_some() || state.modal.is_some() {
        let overlay = container(modal(state))
            .padding(28)
            .center_x(Fill)
            .center_y(Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgba(0., 0., 0., 0.68).into()),
                ..Default::default()
            });
        // An opaque input surface keeps pointer events from reaching the workspace.
        stack![base, widget::opaque(overlay)].into()
    } else {
        base.into()
    }
}
fn nav(
    label: &'static str,
    message: Message,
    selected: bool,
    enabled: bool,
) -> widget::Button<'static, Message> {
    button(text(label).size(14))
        .padding([12, 10])
        .width(Fill)
        .on_press_maybe(enabled.then_some(message))
        .style(move |theme, status| {
            if selected {
                button::primary(theme, status)
            } else {
                button::text(theme, status)
            }
        })
}
fn workspace(state: &Desktop) -> Element<'_, Message> {
    let app = &state.app;
    let idle = !app.busy();
    let color = app
        .source
        .as_ref()
        .is_some_and(|s| s.info["format"] == "Game Boy Color");
    let hero = row![
        column![
            heading("YOUR CARTRIDGE, PRESERVED"),
            text(app.platform.to_string()).size(30),
            text(app.platform.connector()).size(14).style(secondary)
        ]
        .spacing(7),
        space().width(Fill),
        platform_logo(app.platform, color)
    ]
    .align_y(Vertical::Center)
    .spacing(16);
    let mut profiles = pick_list(app.platform.profiles(), Some(app.profile), Message::Profile)
        .text_size(14)
        .width(Fill);
    if !idle {
        profiles = pick_list(
            Vec::<cartridge_app::Profile>::new(),
            Some(app.profile),
            Message::Profile,
        )
        .text_size(14)
        .width(Fill);
    }
    let board = card(
        column![
            row![
                column![heading("PHYSICAL BOARD"), profiles]
                    .spacing(8)
                    .width(Fill),
                control(
                    if app.profile.id == "auto" {
                        "Detect"
                    } else {
                        "Inspect"
                    },
                    Message::Action("probe"),
                    app.can("probe")
                )
            ]
            .spacing(16)
            .align_y(Vertical::Bottom),
            text(app.board_text()).size(12).style(secondary),
            text(app.reader_note()).size(12).style(secondary)
        ]
        .spacing(10),
    );
    let source: Element<'_, Message> = if let Some(source) = &app.source {
        let info = &source.info;
        column![
            heading("LOADED ROM"),
            text(source.title()).size(23),
            text(format!(
                "{}  ·  {} KiB  ·  {}",
                value_text(&info["format"]),
                info["bytes"].as_u64().unwrap_or(0) / 1024,
                value_text(&info["mapper"])
            ))
            .size(14),
            text(value_text(&info["checksum"]))
                .size(13)
                .style(secondary),
            text(source.path.display().to_string())
                .size(12)
                .style(secondary),
            space().height(6),
            row![
                control("Checksums", Message::Hashes, idle),
                control("Save a copy", Message::SaveRom, idle)
            ]
            .spacing(8)
        ]
        .spacing(12)
        .into()
    } else {
        column![heading("LOADED ROM"), text("A place for your next backup").size(23), text("Read your cartridge, drop a ROM here, or browse your files. Game details appear when its exact hash is known.").size(15).style(secondary), space().height(10), control("Browse ROMs", Message::Load, idle), text("GAME BOY  ·  GAME BOY COLOR  ·  NES").size(10).style(secondary)].spacing(14).into()
    };
    let mut game = column![heading("GAME IDENTITY"), text(app.game_text()).size(13)].spacing(8);
    if let Some(source) = &app.source {
        let artwork = &source.info["game"]["artwork"];
        let mut pictures = row![].spacing(8);
        for (key, title) in [
            ("boxart", "Box art"),
            ("screenshot", "Gameplay"),
            ("title_screen", "Title screen"),
        ] {
            if let Some(path) = artwork[key]["path"].as_str() {
                pictures = pictures.push(
                    column![
                        image(image::Handle::from_path(path))
                            .width(Fill)
                            .height(92)
                            .content_fit(iced::ContentFit::Contain),
                        text(title).size(10).style(secondary)
                    ]
                    .spacing(4)
                    .width(FillPortion(1)),
                );
            }
        }
        game = game.push(pictures);
        if source.info["game"]["status"] == "identified" {
            game = game.push(control(
                "Refresh artwork",
                Message::FetchArtwork,
                app.can("game"),
            ));
        }
        game = game.push(
            text("Catalog: Libretro · exact hash and size\nArtwork: Libretro thumbnails")
                .size(10)
                .style(secondary),
        );
    }
    let gba_size: Element<'_, Message> = if app.platform == Platform::Gba {
        row![
            text("GBA ROM size").size(13),
            pick_list(
                if idle {
                    cartridge_app::GbaSize::ALL.to_vec()
                } else {
                    vec![]
                },
                Some(app.settings.gba_size),
                Message::GbaSize
            )
            .text_size(13)
        ]
        .spacing(12)
        .align_y(Vertical::Center)
        .into()
    } else {
        space().height(0).into()
    };
    let opts = card(column![heading("READ VERIFICATION"), gba_size, row![checkbox(app.settings.double_read).label("Read twice and compare").on_toggle_maybe(idle.then_some(Message::Double)).text_size(13), checkbox(app.settings.strict_checksum).label("Strict GB checksums").on_toggle_maybe(idle.then_some(Message::Strict)).text_size(13)].spacing(24), text("Backups and write verification always use mandatory checks. Save RAM and RTC are not included.").size(11).style(secondary)].spacing(10));
    let main = scrollable(
        column![
            hero,
            board,
            row![
                card(source).width(FillPortion(1)).height(Fill),
                card(game).width(FillPortion(1)).height(Fill)
            ]
            .spacing(16)
            .height(310),
            opts
        ]
        .spacing(18)
        .padding([20, 24]),
    )
    .height(Fill);
    let actions = row![
        primary("Read", Message::Action("read"), app.can("read")),
        control("Backup", Message::Action("backup"), app.can("backup")),
        control("Verify", Message::Action("verify"), app.can("verify")),
        control("Check ROM", Message::Action("check"), app.can("check")),
        space().width(Fill),
        control("Write…", Message::Action("write"), app.can("write")),
        control("Wipe…", Message::Action("wipe"), app.can("wipe"))
    ]
    .spacing(8)
    .align_y(Vertical::Center);
    let status: Element<'_, Message> = if let Some(error) = &app.error {
        row![
            column![
                text(&error.message).size(15).style(error_text),
                text(&error.action).size(12)
            ]
            .spacing(6)
            .width(Fill),
            control("Details", Message::ErrorDetails, true)
        ]
        .spacing(14)
        .align_y(Vertical::Center)
        .into()
    } else {
        let mut status = row![column![
            text(if app.busy() {
                "Operation in progress"
            } else {
                "Ready"
            })
            .size(14)
            .style(accent_text),
            text(&app.status).size(12)
        ]
        .spacing(6)
        .width(Fill)]
        .spacing(14)
        .align_y(Vertical::Center);
        if app.busy() {
            status = status.push(control(
                if app.stopping() {
                    "Stopping…"
                } else {
                    "Stop"
                },
                Message::Stop,
                !app.stopping(),
            ));
        }
        status.into()
    };
    column![
        main,
        widget::rule::horizontal(1),
        column![
            actions,
            text(app.write_note()).size(12).style(secondary),
            status
        ]
        .spacing(12)
        .padding([18, 24])
    ]
    .height(Fill)
    .into()
}
fn history(state: &Desktop) -> Element<'_, Message> {
    let mut rows = column![text("Every operation leaves a record").size(28), text("Backups, source files and verification reports stay in your library. Failed operations are retained too.").size(14).style(secondary)].spacing(16).padding(24);
    if state.app.history.is_empty() {
        rows = rows.push(card(text("Your first backup will appear here.")));
    }
    for h in &state.app.history {
        // Reports are opened inside the app; loading a retained ROM uses the same inspection path.
        let content = serde_json::to_string_pretty(&h.report).unwrap_or_default();
        let mut actions = row![
            control("Report", Message::Report(content.clone()), true),
            control("Copy report", Message::Copy(content), true)
        ];
        if let Some(path) = h.report["output"].as_str() {
            actions = actions.push(control(
                "Load ROM",
                Message::FileOpen(path.into()),
                !state.app.busy(),
            ));
        }
        rows = rows.push(card(
            column![
                text(
                    h.path
                        .parent()
                        .unwrap()
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                )
                .size(15),
                text(format!(
                    "{} · {}",
                    value_text(&h.report["operation"]),
                    value_text(&h.report["status"])
                ))
                .size(13)
                .style(secondary),
                actions.spacing(8)
            ]
            .spacing(10),
        ));
    }
    scrollable(rows).height(Fill).into()
}
fn settings(state: &Desktop) -> Element<'_, Message> {
    let settings = &state.app.settings;
    let idle = !state.app.busy();
    scrollable(column![text("Make room for your collection").size(28), card(column![heading("LIBRARY FOLDER"), text_input("Absolute path to an existing folder", &state.library_path).on_input(Message::Library).padding(12).on_submit(Message::SaveSettings), text("Backups and reports are saved here. Changing this setting does not move previous files.").size(13).style(secondary), control("Save location", Message::SaveSettings, idle)].spacing(12)), card(column![heading("PREFERENCES"), checkbox(settings.double_read).label("Read cartridges twice and compare").on_toggle_maybe(idle.then_some(Message::Double)), checkbox(settings.strict_checksum).label("Require valid Game Boy checksums for ordinary reads").on_toggle_maybe(idle.then_some(Message::Strict)), checkbox(settings.download_artwork).label("Download artwork after a known game is read").on_toggle_maybe(idle.then_some(Message::Artwork)), checkbox(settings.color_scheme == 2).label("Light appearance").on_toggle_maybe(idle.then_some(Message::Light)), row![text("GBA read size").size(14), pick_list(if idle { cartridge_app::GbaSize::ALL.to_vec() } else { vec![] }, Some(settings.gba_size), Message::GbaSize)].spacing(16).align_y(Vertical::Center), text("Automatic GBA sizes from the catalog require an exact ROM hash. Choose 32 MiB to preserve the full address window. GBA header checks always apply.").size(12).style(secondary), text("Identification works offline. Artwork uses HTTPS and is cached locally.").size(13).style(secondary)].spacing(18)), card(column![heading("ABOUT CARTRIDGE STUDIO"), text(format!("Version {VERSION} · Rust core, desktop and terminal")), text("The desktop uses Iced; the terminal uses Ratatui. Controls and the offline catalog are built into the package. No separate Python, GTK or browser installation is needed.").size(14), text("Nintendo platform marks identify supported hardware. This independent application is not affiliated with Nintendo.").size(12).style(secondary)].spacing(12))].spacing(20).padding(24)).height(Fill).into()
}
fn modal(state: &Desktop) -> Element<'_, Message> {
    if let Some(review) = &state.app.review {
        return dialog(
            "Review cartridge change",
            column![
                scrollable(text(review.text()).size(14)).height(Fill),
                text_input(
                    &format!("Type {}", review.action().to_uppercase()),
                    &review.word
                )
                .on_input(Message::ReviewWord)
                .on_submit_maybe(review.ready().then_some(Message::Approve))
                .padding(12),
                row![
                    control("Cancel", Message::CloseModal, true),
                    space().width(Fill),
                    primary("Confirm cartridge change", Message::Approve, review.ready())
                ]
                .spacing(12)
            ],
        )
        .into();
    }
    match state.modal.as_ref().unwrap() {
        Modal::Files(browser) => {
            let rows = browser.entries.iter().fold(Column::new().spacing(4), |rows, entry| rows.push(button(text(entry.label()).size(14)).width(Fill).padding([9, 12]).style(button::text).on_press(Message::FileOpen(entry.path.clone()))));
            dialog("Load a ROM", column![row![control("Parent folder", Message::FileUp, browser.folder.parent().is_some()), text_input("Path to a folder or ROM", &browser.path).on_input(Message::FilePath).on_submit(Message::FileSubmit).padding(12)].spacing(10), scrollable(rows).height(Fill), text(&browser.note).size(12).style(secondary), row![control("Cancel", Message::CloseModal, true), space().width(Fill), primary("Open path", Message::FileSubmit, true)]])
        },
        Modal::Hashes { expected } => dialog("File checksums", column![scrollable(text(state.app.hashes()).size(14)).height(Fill), text_input("Expected CRC32, SHA-1 or SHA-256", expected).on_input(Message::Expected).on_submit(Message::Compare).padding(12), row![control("Close", Message::CloseModal, true), control("Copy", Message::Copy(state.app.hashes()), true), control("Export", Message::ExportText(state.app.hashes()), true), space().width(Fill), primary("Compare", Message::Compare, !expected.trim().is_empty() && state.app.can("checksum"))].spacing(8)]),
        Modal::Text { title, content } => dialog(title, column![scrollable(text(content).size(14)).height(Fill), row![control("Close", Message::CloseModal, true), space().width(Fill), control("Copy", Message::Copy(content.clone()), true), primary("Export", Message::ExportText(content.clone()), true)].spacing(8)]),
        Modal::Stop => dialog("Stop this operation?", column![text("Completed backups and partial reads will be retained. If erasing or writing has begun, restore the retained source ROM before using the cartridge.\n\nKeep USB connected while the service finishes cleanup.").size(16), space().height(Fill), row![control("Continue operation", Message::CloseModal, true), space().width(Fill), primary("Stop safely", Message::ConfirmStop, true)]]),
        Modal::Save { path } => dialog("Save a ROM copy", column![text("Enter the full destination filename. Existing files are never overwritten; the copy must match the loaded SHA-256.").size(16), text_input("Full destination path", path).on_input(Message::SavePath).on_submit(Message::SaveCopy).padding(12), space().height(Fill), row![control("Cancel", Message::CloseModal, true), space().width(Fill), primary("Save copy", Message::SaveCopy, !path.trim().is_empty())]]),
    }.into()
}
fn dialog<'a>(title: &'a str, body: Column<'a, Message>) -> widget::Container<'a, Message> {
    card(column![text(title).size(26), body.spacing(18).height(Fill)].spacing(24))
        .max_width(820)
        .height(540)
        .padding(28)
}
