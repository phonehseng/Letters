//! the macos menu items of conversion and export, the ai tools and
//! settings. like every menu item they send their command's name to the
//! window in front (see menu.rs); the ui decides what it does there.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use tauri::menu::{MenuItem, MenuItemBuilder};
use tauri::{AppHandle, Wry};

/// (id, label, shortcut) of each item.
const APP_ITEMS: [(&str, &str, Option<&str>); 1] =
    [("open-settings", "Settings…", Some("CmdOrCtrl+,"))];

const FILE_ITEMS: [(&str, &str, Option<&str>); 3] = [
    // any file to another type, without opening it (export/ConvertSheet.svelte)
    ("convert-file", "Convert File…", None),
    ("export-to", "Export To…", Some("CmdOrCtrl+Shift+E")),
    ("open-pdf-as-document", "Open PDF as Document…", None),
];

const VIEW_ITEMS: [(&str, &str, Option<&str>); 2] = [
    ("find-topic", "Find a Topic…", Some("CmdOrCtrl+Shift+F")),
    // only does something while AI Tools are on in Settings
    ("toggle-ai-tools", "AI Tools", None),
];

fn items(
    app: &AppHandle,
    list: &[(&str, &str, Option<&str>)],
) -> tauri::Result<Vec<MenuItem<Wry>>> {
    list.iter()
        .map(|(id, label, shortcut)| {
            let item = MenuItemBuilder::with_id(*id, *label);
            match shortcut {
                Some(keys) => item.accelerator(*keys).build(app),
                None => item.build(app),
            }
        })
        .collect()
}

/// Settings…, for the app menu under About.
pub fn app_items(app: &AppHandle) -> tauri::Result<Vec<MenuItem<Wry>>> {
    items(app, &APP_ITEMS)
}

/// Convert File…, Export To… and Open PDF as Document…, for the File menu.
pub fn file_items(app: &AppHandle) -> tauri::Result<Vec<MenuItem<Wry>>> {
    items(app, &FILE_ITEMS)
}

/// Find a Topic… and AI Tools, for the View menu.
pub fn view_items(app: &AppHandle) -> tauri::Result<Vec<MenuItem<Wry>>> {
    items(app, &VIEW_ITEMS)
}

pub fn is_conversion_command(id: &str) -> bool {
    APP_ITEMS
        .iter()
        .chain(&FILE_ITEMS)
        .chain(&VIEW_ITEMS)
        .any(|(item, _, _)| *item == id)
}
