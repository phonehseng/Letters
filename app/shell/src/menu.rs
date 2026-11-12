//! the native menu bar, used on macos only.
//!
//! on macos every app has a menu bar at the top of the screen, and people
//! expect each command to be in it with its keyboard shortcut. on windows
//! a menu is drawn as a bar inside every window, which would break the
//! floating toolbar design (see docs/DESIGN.md), so there the ui handles
//! the same shortcuts itself and this menu is never attached.
//!
//! picking a menu item sends the event `menu-command` to the focused
//! window only, with the command's name ("zoom-in", "find", ...) as the
//! payload. the window's ui then does exactly what its own toolbar
//! button or shortcut would do.

// the menu is built only on macos, but the code is compiled everywhere
// so it is checked on every machine
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use tauri::menu::{Menu, MenuBuilder, MenuEvent, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, EventTarget, Manager, WebviewWindow, Wry};

use crate::windows;

/// the event name the ui listens for.
pub const MENU_COMMAND_EVENT: &str = "menu-command";

/// closes the focused window here in the shell; it is not sent to the ui.
const CLOSE_WINDOW: &str = "close-window";
/// opens a window even when none is focused (see `handle_event`).
const NEW_WINDOW: &str = "new-window";

/// a menu entry whose id is also the payload sent to the window.
struct Command {
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
}

const fn command(id: &'static str, label: &'static str, shortcut: &'static str) -> Command {
    Command {
        id,
        label,
        shortcut: Some(shortcut),
    }
}

const FILE_COMMANDS: [Command; 2] = [
    command("open", "Open…", "CmdOrCtrl+O"),
    command(NEW_WINDOW, "New Window", "CmdOrCtrl+N"),
];

const SAVE_COMMANDS: [Command; 2] = [
    command("save", "Save", "CmdOrCtrl+S"),
    command("save-as", "Save As…", "CmdOrCtrl+Shift+S"),
];

/// undo and redo are our own items, not the standard ones.
///
/// the standard items send undo straight to the web view, which only
/// knows about typing in text fields and nothing about document edits.
/// our items send "undo" and "redo" to the window like every other
/// command, and the ui decides: in a text field it undoes the typing
/// (as the standard item would), anywhere else it undoes the last edit
/// to the document. cut, copy, paste and select all stay standard,
/// because only text fields use them.
const UNDO_COMMANDS: [Command; 2] = [
    command("undo", "Undo", "CmdOrCtrl+Z"),
    command("redo", "Redo", "CmdOrCtrl+Shift+Z"),
];

/// the same keys as Preview's Rotate Left and Rotate Right.
const TOOL_COMMANDS: [Command; 2] = [
    command("rotate-left", "Rotate Left", "CmdOrCtrl+L"),
    command("rotate-right", "Rotate Right", "CmdOrCtrl+R"),
];

const ZOOM_COMMANDS: [Command; 3] = [
    command("zoom-in", "Zoom In", "CmdOrCtrl+="),
    command("zoom-out", "Zoom Out", "CmdOrCtrl+-"),
    command("fit-width", "Fit Width", "CmdOrCtrl+0"),
];

const PANEL_COMMANDS: [Command; 5] = [
    command("find", "Find…", "CmdOrCtrl+F"),
    command("toggle-thumbnails", "Thumbnails", "CmdOrCtrl+Alt+1"),
    command("toggle-outline", "Outline", "CmdOrCtrl+Alt+2"),
    command("toggle-two-pages", "Two Pages", "CmdOrCtrl+Alt+3"),
    Command {
        id: "toggle-night-mode",
        label: "Night Mode",
        shortcut: None,
    },
];

/// builds the whole menu bar: the app menu, File, Edit, View and Tools.
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let app_menu = SubmenuBuilder::new(app, "Letters")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let close_window = MenuItemBuilder::with_id(CLOSE_WINDOW, "Close Window")
        .accelerator("CmdOrCtrl+W")
        .build(app)?;
    let file_menu = SubmenuBuilder::new(app, "File")
        .items(&as_items(&menu_items(app, &FILE_COMMANDS)?))
        .separator()
        .item(&close_window)
        .items(&as_items(&menu_items(app, &SAVE_COMMANDS)?))
        .build()?;

    // the standard cut, copy, paste and select all items are what make
    // them work in text fields on macos. undo and redo are ours (see
    // UNDO_COMMANDS for why)
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .items(&as_items(&menu_items(app, &UNDO_COMMANDS)?))
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let tools_menu = SubmenuBuilder::new(app, "Tools")
        .items(&as_items(&menu_items(app, &TOOL_COMMANDS)?))
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .items(&as_items(&menu_items(app, &ZOOM_COMMANDS)?))
        .separator()
        .items(&as_items(&menu_items(app, &PANEL_COMMANDS)?))
        .build()?;

    MenuBuilder::new(app)
        .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &tools_menu])
        .build()
}

/// reacts to a picked menu item.
pub fn handle_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    let Some(window) = focused_window(app) else {
        // with no window in front, a new window is the one command that
        // still makes sense
        if id == NEW_WINDOW {
            windows::open_window(app, None);
        }
        return;
    };
    if id == CLOSE_WINDOW {
        let _ = window.close();
    } else if is_window_command(id) {
        // aimed at one window: the other windows must not also zoom
        let target = EventTarget::webview_window(window.label());
        let _ = app.emit_to(target, MENU_COMMAND_EVENT, id);
    }
}

fn menu_items(app: &AppHandle, commands: &[Command]) -> tauri::Result<Vec<MenuItem<Wry>>> {
    commands
        .iter()
        .map(|command| {
            let item = MenuItemBuilder::with_id(command.id, command.label);
            match command.shortcut {
                Some(shortcut) => item.accelerator(shortcut).build(app),
                None => item.build(app),
            }
        })
        .collect()
}

/// the builders want a list of borrowed menu items of any kind.
fn as_items(items: &[MenuItem<Wry>]) -> Vec<&dyn tauri::menu::IsMenuItem<Wry>> {
    items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect()
}

fn is_window_command(id: &str) -> bool {
    FILE_COMMANDS
        .iter()
        .chain(&SAVE_COMMANDS)
        .chain(&UNDO_COMMANDS)
        .chain(&ZOOM_COMMANDS)
        .chain(&PANEL_COMMANDS)
        .chain(&TOOL_COMMANDS)
        .any(|command| command.id == id)
}

fn focused_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false))
}
