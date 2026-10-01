use super::*;
use iced_test::Simulator;

#[test]
fn redraw_notifications_do_not_trigger_an_idle_render_loop() {
    assert!(presentation_event(
        Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
        event::Status::Ignored
    )
    .is_none());
    assert!(presentation_event(
        Event::Window(window::Event::Resized(Size::new(800., 500.))),
        event::Status::Ignored
    )
    .is_some());
}

#[test]
fn gbxcart_flash_controls_explain_profile_and_source_requirements() {
    let root = tempfile::tempdir().unwrap();
    let mut desktop = state(root.path());
    desktop.size = Size::new(1000., 700.);
    desktop.app.no_device = false;
    desktop.app.connected = true;
    desktop.app.connected_reader = Some(cartridge_app::ReaderKind::Gbxcart);
    assert!(!desktop.app.can("write"));
    let mut ui = Simulator::with_size(iced::Settings::default(), desktop.size, desktop.view());
    assert!(ui.find(desktop.app.write_note()).is_ok());
    drop(ui);
    desktop
        .app
        .set_profile(Platform::GameBoy.profiles()[2])
        .unwrap();
    assert!(desktop.app.can("wipe"));
    assert!(!desktop.app.can("write"));
    desktop.app.source = Some(cartridge_app::model::Source {
        // This is display-only fixture data; keep the rendered path deterministic.
        path: std::path::PathBuf::from("/cartridge-studio-test/homebrew.gbc"),
        info: serde_json::json!({"platform":"gameboy","title":"HOMEBREW","format":"Game Boy Color","mapper":"MBC5","bytes":262144,"hashes":{"sha256":"0".repeat(64)}}),
    });
    assert!(desktop.app.can("write"));
    let mut ui = Simulator::with_size(iced::Settings::default(), desktop.size, desktop.view());
    assert!(ui.find(desktop.app.write_note()).is_ok());
    let out =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/rust-native-qa/headless");
    std::fs::create_dir_all(&out).unwrap();
    assert!(ui
        .snapshot(&desktop.theme())
        .unwrap()
        .matches_image(out.join(format!(
            "gbxcart-spansion-write-v{}-1000-700",
            env!("CARGO_PKG_VERSION").replace('.', "-")
        )))
        .unwrap());
}

fn state(root: &std::path::Path) -> Desktop {
    Desktop::new(&Options {
        library: Some(root.into()),
        rom: None,
        no_device: true,
        size: Size::new(1280., 860.),
    })
}
fn click(state: &mut Desktop, target: &str) {
    let mut ui = Simulator::with_size(iced::Settings::default(), state.size, state.view());
    ui.click(target).unwrap();
    let messages = ui.into_messages().collect::<Vec<_>>();
    for message in messages {
        let _ = state.update(message);
    }
}
#[test]
fn actual_buttons_open_files_preferences_and_checksum_dialogs() {
    let root = tempfile::tempdir().unwrap();
    let mut app = state(root.path());
    click(&mut app, "NES");
    assert_eq!(app.app.platform, Platform::Nes);
    click(&mut app, "Famicom");
    assert_eq!(app.app.platform, Platform::Famicom);
    click(&mut app, "Game Boy Advance");
    assert_eq!(app.app.platform, Platform::Gba);
    click(&mut app, "Load ROM");
    assert!(matches!(app.modal, Some(Modal::Files(_))));
    click(&mut app, "Cancel");
    assert!(app.modal.is_none());
    click(&mut app, "Preferences");
    assert_eq!(app.app.page, Page::Settings);
    click(&mut app, "Light appearance");
    assert_eq!(app.app.settings.color_scheme, 2);
    click(&mut app, "Game Boy / Color");
    assert_eq!(app.app.page, Page::Workspace);
    click(&mut app, "Write…");
    assert!(app.app.review.is_none());
    assert!(!app.app.busy());
    click(&mut app, "Wipe…");
    assert!(app.app.review.is_none());
}
#[test]
fn small_window_hides_workspace_and_restores_pending_dialog() {
    let root = tempfile::tempdir().unwrap();
    let mut app = state(root.path());
    click(&mut app, "Load ROM");
    let _ = app.update(Message::Event(Event::Window(window::Event::Resized(
        Size::new(900., 600.),
    ))));
    let mut ui = Simulator::with_size(iced::Settings::default(), app.size, app.view());
    assert!(ui.find("Window too small").is_ok());
    assert!(ui.find("Load ROM").is_err());
    drop(ui);
    let _ = app.update(Message::Action("write"));
    assert!(!app.app.busy());
    let _ = app.update(Message::Event(Event::Window(window::Event::Resized(
        Size::new(1280., 860.),
    ))));
    assert!(matches!(app.modal, Some(Modal::Files(_))));
}
#[test]
fn confirmation_dialog_blocks_background_actions_and_escape_cancels() {
    let root = tempfile::tempdir().unwrap();
    let mut app = state(root.path());
    app.app.no_device = false;
    app.app.connected = true;
    app.app.set_profile(app.app.platform.profiles()[1]).unwrap();
    click(&mut app, "Wipe…");
    assert!(app.app.review.is_some());
    click(&mut app, "Confirm cartridge change");
    assert!(!app.app.busy());
    click(&mut app, "Cancel");
    assert!(app.app.review.is_none());
    assert!(!app.app.busy());
}
#[test]
fn render_all_pages_and_dialogs_at_supported_sizes() {
    let root = tempfile::tempdir().unwrap();
    let mut app = state(root.path());
    let out =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/rust-native-qa/headless");
    std::fs::create_dir_all(&out).unwrap();
    if let Ok(path) = std::env::var("CARTRIDGE_STUDIO_TEST_ROM_INFO") {
        let info: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        app.app.source = Some(cartridge_app::Source {
            path: info["path"].as_str().unwrap().into(),
            info,
        });
    } else {
        app.app.source = Some(cartridge_app::Source {
            path: root.path().join("fixture.gb"),
            info: serde_json::json!({"title":"TEST CARTRIDGE","format":"Game Boy","platform":"gameboy","bytes":65536,"mapper":"MBC1","checksum":"Header, size and global checksum passed.","hashes":{"sha256":"0".repeat(64),"sha1":"1".repeat(40),"crc32":"12345678"},"game":{"status":"not_found","message":"No exact catalog match."}}),
        });
    }
    for (w, h) in [(1000, 700), (1280, 860), (800, 500)] {
        app.size = Size::new(w as f32, h as f32);
        for (name, page) in [
            ("workspace", Page::Workspace),
            ("history", Page::History),
            ("settings", Page::Settings),
            ("support", Page::Support),
        ] {
            app.app.page = page;
            let mut ui = Simulator::with_size(iced::Settings::default(), app.size, app.view());
            let snapshot = ui.snapshot(&app.theme()).unwrap();
            let path = out.join(format!("{name}-{w}-{h}-{}", std::process::id()));
            assert!(snapshot.matches_image(path).unwrap());
        }
    }
    app.size = Size::new(1000., 700.);
    app.app.page = Page::Workspace;
    for slot in Platform::ALL {
        app.app.set_platform(slot).unwrap();
        let mut ui = Simulator::with_size(iced::Settings::default(), app.size, app.view());
        assert!(ui.find(slot.connector()).is_ok());
        let snapshot = ui.snapshot(&app.theme()).unwrap();
        assert!(snapshot
            .matches_image(out.join(format!("slot-{}-{}", slot.id(), std::process::id())))
            .unwrap());
    }
    for (name, modal) in [
        (
            "hashes",
            Modal::Hashes {
                expected: "a".repeat(64),
            },
        ),
        ("files", Modal::Files(Browser::new(root.path()).unwrap())),
        ("stop", Modal::Stop),
        (
            "error",
            Modal::Text {
                title: "Action needed".into(),
                content: "An actionable error with a retained backup path\n".repeat(30),
            },
        ),
    ] {
        app.modal = Some(modal);
        let mut ui = Simulator::with_size(iced::Settings::default(), app.size, app.view());
        assert!(ui
            .snapshot(&app.theme())
            .unwrap()
            .matches_image(out.join(format!("{name}-{}", std::process::id())))
            .unwrap());
    }
}

#[test]
fn modal_blocks_queued_workspace_actions_and_light_theme_renders() {
    let root = tempfile::tempdir().unwrap();
    let mut app = state(root.path());
    click(&mut app, "Load ROM");
    let _ = app.update(Message::Platform(Platform::Famicom));
    assert_eq!(app.app.platform, Platform::GameBoy);
    let _ = app.update(Message::Action("read"));
    assert!(!app.app.busy());
    let _ = app.update(Message::CloseModal);
    app.app.settings.color_scheme = 2;
    let mut ui = Simulator::with_size(iced::Settings::default(), app.size, app.view());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../tmp/rust-native-qa/light-{}",
        std::process::id()
    ));
    assert!(ui
        .snapshot(&app.theme())
        .unwrap()
        .matches_image(path)
        .unwrap());
}

#[test]
fn gbxcart_selector_and_capabilities_fit_the_minimum_window() {
    let root = tempfile::tempdir().unwrap();
    let mut app = state(root.path());
    app.size = Size::new(1000., 700.);
    let _ = app.update(Message::Reader(cartridge_app::ReaderKind::Gbxcart));
    assert_eq!(app.app.settings.reader, cartridge_app::ReaderKind::Gbxcart);
    assert!(!app.app.connected);
    app.app.no_device = false;
    app.app.connected = true;
    app.app.connected_reader = Some(cartridge_app::ReaderKind::Gbxcart);
    app.app.reader = "GBxCart RW · L14".into();
    assert!(app.app.can("read"));
    click(&mut app, "Write…");
    click(&mut app, "Wipe…");
    assert!(app.app.review.is_none());
    let mut ui = Simulator::with_size(iced::Settings::default(), app.size, app.view());
    assert!(ui.find("GBxCart RW · L14").is_ok());
    let out =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/rust-native-qa/headless");
    std::fs::create_dir_all(&out).unwrap();
    assert!(ui
        .snapshot(&app.theme())
        .unwrap()
        .matches_image(out.join(format!("gbxcart-{}", std::process::id())))
        .unwrap());
}

#[test]
fn gba_size_changes_are_blocked_by_modal_and_reader_capabilities_apply() {
    let root = tempfile::tempdir().unwrap();
    let mut app = state(root.path());
    click(&mut app, "Game Boy Advance");
    app.app
        .set_reader(cartridge_app::ReaderKind::Gbxcart)
        .unwrap();
    app.app.no_device = false;
    app.app.connected = true;
    assert!(app.app.can("read"));
    assert!(!app.app.can("wipe"));
    let _ = app.update(Message::GbaSize(cartridge_app::GbaSize::M32));
    assert_eq!(app.app.settings.gba_size, cartridge_app::GbaSize::M32);
    click(&mut app, "Load ROM");
    let _ = app.update(Message::GbaSize(cartridge_app::GbaSize::M1));
    assert_eq!(app.app.settings.gba_size, cartridge_app::GbaSize::M32);
}
