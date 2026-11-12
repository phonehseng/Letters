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

/// compare and accessibility, in the File menu: both look at the whole
/// document. next and previous change use Option with the arrows, as the
/// Compare screen does (it sends them on as next-change and previous-change)
const COMPARE_COMMANDS: [Command; 5] = [
    plain("compare-with-version", "Compare with Another Version…"),
    plain("compare-two-pdfs", "Compare Two PDFs…"),
    plain("next-change", "Next Change"),
    plain("previous-change", "Previous Change"),
    plain("check-accessibility", "Check Accessibility…"),
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

/// a menu entry without a key of its own.
const fn plain(id: &'static str, label: &'static str) -> Command {
    Command {
        id,
        label,
        shortcut: None,
    }
}

/// the Pages menu. rotate uses the same keys as Preview. Delete has no
/// key here on purpose: a menu key would take the delete key away from
/// every text field, so the ui handles it in the thumbnails instead.
/// items with an ellipsis open the Tools inspector at that tool.
const PAGE_COMMANDS: [Command; 4] = [
    command("rotate-left", "Rotate Left", "CmdOrCtrl+L"),
    command("rotate-right", "Rotate Right", "CmdOrCtrl+R"),
    command("move-pages-up", "Move Pages Up", "Alt+Up"),
    command("move-pages-down", "Move Pages Down", "Alt+Down"),
];

const PAGE_CHANGE_COMMANDS: [Command; 4] = [
    plain("delete-pages", "Delete Pages"),
    plain("insert-blank-page", "Insert Blank Page"),
    plain("insert-from-pdf", "Insert Pages from PDF…"),
    plain("crop", "Crop…"),
];

const PAGE_FILE_COMMANDS: [Command; 3] = [
    plain("extract-pages", "Extract Pages…"),
    plain("split", "Split…"),
    plain("merge", "Merge PDFs…"),
];

/// the Annotate menu. the text marks and the note use Preview's keys
/// (control and command with a letter), so the keys a mac user already
/// knows work here too. the ui accepts ctrl and alt with the same letter
/// on windows. Delete has no key here for the same reason as in Pages:
/// the ui deletes the selected comment when its frame has the focus.
const ANNOTATE_MARK_COMMANDS: [Command; 3] = [
    command("highlight", "Highlight", "Ctrl+Cmd+H"),
    command("underline", "Underline", "Ctrl+Cmd+U"),
    command("strike-through", "Strike Through", "Ctrl+Cmd+S"),
];

const ANNOTATE_ADD_COMMANDS: [Command; 6] = [
    command("add-note", "Add Note", "Ctrl+Cmd+N"),
    command("add-text-box", "Add Text Box", "Ctrl+Cmd+T"),
    plain("add-shape", "Add Shape"),
    command("draw", "Draw", "Ctrl+Cmd+D"),
    plain("add-picture", "Add Picture…"),
    plain("select-tool", "Select"),
];

const ANNOTATE_LIST_COMMANDS: [Command; 2] = [
    plain("show-comments", "Show Comments"),
    plain("flatten-comments", "Flatten Comments…"),
];

const ZOOM_COMMANDS: [Command; 3] = [
    command("zoom-in", "Zoom In", "CmdOrCtrl+="),
    command("zoom-out", "Zoom Out", "CmdOrCtrl+-"),
    command("fit-width", "Fit Width", "CmdOrCtrl+0"),
];

const PANEL_COMMANDS: [Command; 6] = [
    command("find", "Find…", "CmdOrCtrl+F"),
    command("toggle-thumbnails", "Thumbnails", "CmdOrCtrl+Alt+1"),
    command("toggle-outline", "Outline", "CmdOrCtrl+Alt+2"),
    command("toggle-two-pages", "Two Pages", "CmdOrCtrl+Alt+3"),
    // the same key as Show Inspector in Pages and Keynote
    command("toggle-inspector", "Tools", "CmdOrCtrl+Alt+I"),
    Command {
        id: "toggle-night-mode",
        label: "Night Mode",
        shortcut: None,
    },
];

/// builds the whole menu bar: the app menu, File, Edit, View, Pages and Annotate.
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
        // scan: New from Images…, see content_menu.rs
        .items(&as_items(&crate::content_menu::file_items(app)?))
        .separator()
        // compare and accessibility (compare_commands.rs, a11y_commands.rs)
        .items(&as_items(&menu_items(app, &COMPARE_COMMANDS)?))
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

    let pages_menu = SubmenuBuilder::new(app, "Pages")
        .items(&as_items(&menu_items(app, &PAGE_COMMANDS)?))
        .separator()
        .items(&as_items(&menu_items(app, &PAGE_CHANGE_COMMANDS)?))
        .separator()
        .items(&as_items(&menu_items(app, &PAGE_FILE_COMMANDS)?))
        .build()?;

    let annotate_menu = SubmenuBuilder::new(app, "Annotate")
        .items(&as_items(&menu_items(app, &ANNOTATE_MARK_COMMANDS)?))
        .separator()
        .items(&as_items(&menu_items(app, &ANNOTATE_ADD_COMMANDS)?))
        .separator()
        .items(&as_items(&menu_items(app, &ANNOTATE_LIST_COMMANDS)?))
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .items(&as_items(&menu_items(app, &ZOOM_COMMANDS)?))
        .separator()
        .items(&as_items(&menu_items(app, &PANEL_COMMANDS)?))
        .build()?;

    // the writer's menus (word-processor mode), see writer_menu.rs
    let format_menu = crate::writer_menu::format_menu(app)?;
    let insert_menu = crate::writer_menu::insert_menu(app)?;
    // the Forms menu (Fill & Sign), see forms_menu.rs
    let forms_menu = crate::forms_menu::forms_menu(app)?;
    // the Protect menu (redaction and passwords), see protect_menu.rs
    let protect_menu = crate::protect_menu::protect_menu(app)?;
    // the Content menu (editing page text and pictures, recognising text), see content_menu.rs
    let content_menu = crate::content_menu::content_menu(app)?;

    MenuBuilder::new(app)
        .items(&[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &pages_menu,
            &annotate_menu,
            &format_menu,
            &insert_menu,
            &forms_menu,
            &protect_menu,
            &content_menu,
        ])
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
        .chain(&PAGE_COMMANDS)
        .chain(&PAGE_CHANGE_COMMANDS)
        .chain(&PAGE_FILE_COMMANDS)
        .chain(&ANNOTATE_MARK_COMMANDS)
        .chain(&ANNOTATE_ADD_COMMANDS)
        .chain(&ANNOTATE_LIST_COMMANDS)
        .chain(&COMPARE_COMMANDS)
        .any(|command| command.id == id)
        || crate::writer_menu::is_writer_command(id)
        || crate::forms_menu::is_forms_command(id)
        || crate::protect_menu::is_protect_command(id)
        || crate::content_menu::is_content_command(id)
}

fn focused_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false))
}
