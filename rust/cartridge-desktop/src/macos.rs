//! Native macOS menus, dialogs, and application lifecycle integration.

use cartridge_app::VERSION;
use iced::{stream, Subscription};
use muda::{
    accelerator::{Accelerator, Code, Modifiers},
    AboutMetadata, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use rfd::FileDialog;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
};

const OPEN: &str = "cartridge.open";
const SAVE_COPY: &str = "cartridge.save-copy";
const SETTINGS: &str = "cartridge.settings";
const HELP: &str = "cartridge.help";
const QUIT: &str = "cartridge.quit";
const ROM_EXTENSIONS: [&str; 4] = ["gb", "gbc", "gba", "nes"];

thread_local! {
    // Muda's macOS menu is main-thread-only. Retaining the root menu for the
    // process lifetime also retains all of its native submenu items.
    static MAIN_MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    SaveCopy,
    Settings,
    Help,
    Quit,
}

pub fn install_menu() -> muda::Result<()> {
    let command = Modifiers::META;
    let open = MenuItem::with_id(
        OPEN,
        "Open…",
        true,
        Some(Accelerator::new(Some(command), Code::KeyO)),
    );
    let save_copy = MenuItem::with_id(
        SAVE_COPY,
        "Save a Copy…",
        true,
        Some(Accelerator::new(
            Some(command | Modifiers::SHIFT),
            Code::KeyS,
        )),
    );
    let settings = MenuItem::with_id(
        SETTINGS,
        "Settings…",
        true,
        Some(Accelerator::new(Some(command), Code::Comma)),
    );
    let help = MenuItem::with_id(HELP, "Cartridge Studio Help", true, None);
    let quit = MenuItem::with_id(
        QUIT,
        "Quit Cartridge Studio",
        true,
        Some(Accelerator::new(Some(command), Code::KeyQ)),
    );
    let about = PredefinedMenuItem::about(
        Some("About Cartridge Studio"),
        Some(AboutMetadata {
            name: Some("Cartridge Studio".into()),
            version: Some(VERSION.into()),
            short_version: Some(VERSION.into()),
            copyright: Some("Copyright © 2026 Cartridge Studio contributors".into()),
            credits: Some(
                "Native Rust cartridge preservation tools. See Third-Party Notices in the application bundle for bundled component licenses."
                    .into(),
            ),
            ..Default::default()
        }),
    );
    let app = Submenu::with_items(
        "Cartridge Studio",
        true,
        &[
            &about,
            &PredefinedMenuItem::separator(),
            &settings,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &quit,
        ],
    )?;
    let file = Submenu::with_items(
        "File",
        true,
        &[
            &open,
            &save_copy,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
        ],
    )?;
    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ],
    )?;
    let window = Submenu::with_items(
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::fullscreen(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ],
    )?;
    let help_menu = Submenu::with_items("Help", true, &[&help])?;
    let menu = Menu::with_items(&[&app, &file, &edit, &window, &help_menu])?;
    menu.init_for_nsapp();
    window.set_as_windows_menu_for_nsapp();
    help_menu.set_as_help_menu_for_nsapp();
    MAIN_MENU.with(|slot| *slot.borrow_mut() = Some(menu));
    Ok(())
}

pub fn subscription() -> Subscription<Action> {
    Subscription::run(menu_events)
}

fn menu_events() -> impl iced::futures::Stream<Item = Action> {
    stream::channel(32, async |output| {
        std::thread::spawn(move || {
            let mut output = output;
            while let Ok(event) = MenuEvent::receiver().recv() {
                if let Some(action) = action_for_id(event.id()) {
                    if output.try_send(action).is_err() {
                        break;
                    }
                }
            }
        });
    })
}

fn action_for_id(id: &MenuId) -> Option<Action> {
    match id.as_ref() {
        OPEN => Some(Action::Open),
        SAVE_COPY => Some(Action::SaveCopy),
        SETTINGS => Some(Action::Settings),
        HELP => Some(Action::Help),
        QUIT => Some(Action::Quit),
        _ => None,
    }
}

#[cfg(not(test))]
pub fn choose_rom(start: &Path) -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Open a cartridge ROM")
        .set_directory(dialog_directory(start))
        .add_filter("Cartridge ROM", &ROM_EXTENSIONS)
        .pick_file()
}

#[cfg(not(test))]
pub fn save_rom(start: &Path, file_name: &str) -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Save a cartridge ROM copy")
        .set_directory(dialog_directory(start))
        .set_file_name(file_name)
        .add_filter("Cartridge ROM", &ROM_EXTENSIONS)
        .set_can_create_directories(true)
        .save_file()
}

pub fn choose_library(start: &Path) -> Option<PathBuf> {
    FileDialog::new()
        .set_title("Choose the Cartridge Studio library")
        .set_directory(dialog_directory(start))
        .set_can_create_directories(true)
        .pick_folder()
}

fn dialog_directory(path: &Path) -> &Path {
    if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(Path::new("/"))
    }
}

pub fn hide_application() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).hide(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_menu_ids_route_without_polling() {
        assert_eq!(action_for_id(&MenuId::new(OPEN)), Some(Action::Open));
        assert_eq!(
            action_for_id(&MenuId::new(SETTINGS)),
            Some(Action::Settings)
        );
        assert_eq!(action_for_id(&MenuId::new("system.item")), None);
    }

    #[test]
    fn native_rom_dialog_filters_every_supported_file_family() {
        assert_eq!(ROM_EXTENSIONS, ["gb", "gbc", "gba", "nes"]);
    }
}
