//! the macos menu items of the Edit and Scan tabs: a Content menu (editing
//! page text and pictures, and recognising text), and New from Images… in
//! the File menu.
//!
//! they work like every other menu (see menu.rs): picking an item sends
//! its id to the focused window as a `menu-command`, and the ui does what
//! the tab's own buttons do. items with an ellipsis open the tab at that
//! tool or a sheet, because each of them asks something first.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use tauri::menu::{MenuItem, MenuItemBuilder, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Wry};

struct Item {
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
}

const fn item(id: &'static str, label: &'static str, shortcut: Option<&'static str>) -> Item {
    Item {
        id,
        label,
        shortcut,
    }
}

/// control command W (ctrl alt W on windows): edit the words on the page
const EDIT: [Item; 3] = [
    item("edit-text", "Edit Text and Images", Some("Ctrl+Cmd+W")),
    item("add-page-text", "Add Text", None),
    item("add-page-image", "Add Image…", None),
];

/// control command O (ctrl alt O on windows): ocr
const SCAN: [Item; 3] = [
    item("make-searchable", "Recognize Text…", Some("Ctrl+Cmd+O")),
    item("show-recognized-text", "Show Recognized Text", None),
    item("remove-recognized-text", "Remove Recognized Text…", None),
];

const FILE: [Item; 1] = [item("new-from-images", "New from Images…", None)];

fn items(app: &AppHandle, list: &[Item]) -> tauri::Result<Vec<MenuItem<Wry>>> {
    list.iter()
        .map(|entry| {
            let builder = MenuItemBuilder::with_id(entry.id, entry.label);
            match entry.shortcut {
                Some(shortcut) => builder.accelerator(shortcut).build(app),
                None => builder.build(app),
            }
        })
        .collect()
}

pub fn refs(items: &[MenuItem<Wry>]) -> Vec<&dyn tauri::menu::IsMenuItem<Wry>> {
    items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect()
}

pub fn content_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    SubmenuBuilder::new(app, "Content")
        .items(&refs(&items(app, &EDIT)?))
        .separator()
        .items(&refs(&items(app, &SCAN)?))
        .build()
}

/// New from Images…, for the File menu.
pub fn file_items(app: &AppHandle) -> tauri::Result<Vec<MenuItem<Wry>>> {
    items(app, &FILE)
}

/// true for ids of these items, which go to the focused window.
pub fn is_content_command(id: &str) -> bool {
    [&EDIT[..], &SCAN, &FILE]
        .iter()
        .flat_map(|list| list.iter())
        .any(|entry| entry.id == id)
}
