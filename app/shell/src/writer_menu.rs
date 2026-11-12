//! the writer's Format and Insert menus in the macos menu bar.
//!
//! they work like every other menu (see menu.rs): picking an item sends
//! its id to the focused window as a `menu-command`, and the writer does
//! what its own buttons and shortcuts do. in the pdf viewer they do
//! nothing. items whose keys the pdf side already uses in the menu bar
//! (Command Option 1 to 3, Command L and R) are listed without a key here:
//! on a mac those keys stay with the View and Pages menus.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use tauri::menu::{MenuItem, MenuItemBuilder, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Wry};

struct Item {
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
}

const fn item(id: &'static str, label: &'static str, shortcut: Option<&'static str>) -> Item {
    Item { id, label, shortcut }
}

const TEXT: [Item; 6] = [
    item("format-bold", "Bold", Some("CmdOrCtrl+B")),
    item("format-italic", "Italic", Some("CmdOrCtrl+I")),
    item("format-underline", "Underline", Some("CmdOrCtrl+U")),
    item("format-strike", "Strikethrough", Some("CmdOrCtrl+Shift+X")),
    item("format-superscript", "Superscript", None),
    item("format-subscript", "Subscript", None),
];

const STYLES: [Item; 6] = [
    item("style-title", "Title", None),
    item("style-heading-1", "Heading 1", None),
    item("style-heading-2", "Heading 2", None),
    item("style-heading-3", "Heading 3", None),
    item("style-body", "Body", Some("CmdOrCtrl+Alt+0")),
    item("style-quote", "Quote", None),
];

const PARAGRAPH: [Item; 8] = [
    item("align-left", "Align Left", None),
    item("align-center", "Center", Some("CmdOrCtrl+E")),
    item("align-right", "Align Right", None),
    item("align-justify", "Justify", Some("CmdOrCtrl+J")),
    item("list-bullets", "Bulleted List", Some("CmdOrCtrl+Shift+8")),
    item("list-numbers", "Numbered List", Some("CmdOrCtrl+Shift+7")),
    item("indent-more", "Increase Indent", Some("CmdOrCtrl+]")),
    item("indent-less", "Decrease Indent", Some("CmdOrCtrl+[")),
];

const REVIEW: [Item; 3] = [
    item("find-replace", "Find and Replace…", Some("CmdOrCtrl+H")),
    item("track-changes", "Track Changes", Some("CmdOrCtrl+Shift+E")),
    item("toggle-ruler", "Ruler", None),
];

const INSERT: [Item; 7] = [
    item("insert-table", "Table…", None),
    item("insert-picture", "Picture…", None),
    item("insert-page-break", "Page Break", Some("CmdOrCtrl+Enter")),
    item("insert-footnote", "Footnote", Some("CmdOrCtrl+Alt+F")),
    item("insert-link", "Link…", Some("CmdOrCtrl+K")),
    item("insert-page-number", "Page Number", None),
    item("insert-comment", "Comment", Some("CmdOrCtrl+Alt+M")),
];

const EXPORT: [Item; 6] = [
    item("export-pdf", "Export as PDF…", None),
    // print works for pdfs too: the viewer handles the same command
    item("print", "Print…", Some("CmdOrCtrl+P")),
    item("save-to-drive", "Save to Google Drive…", None),
    item("toggle-strict-mode", "Strict Word Mode", None),
    item("image-inline", "Picture In Line", Some("CmdOrCtrl+Alt+Shift+L")),
    item("image-free", "Picture Free", Some("CmdOrCtrl+Alt+Shift+F")),
];

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

fn refs(items: &[MenuItem<Wry>]) -> Vec<&dyn tauri::menu::IsMenuItem<Wry>> {
    items.iter().map(|item| item as &dyn tauri::menu::IsMenuItem<Wry>).collect()
}

pub fn format_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    SubmenuBuilder::new(app, "Format")
        .items(&refs(&items(app, &TEXT)?))
        .separator()
        .items(&refs(&items(app, &STYLES)?))
        .separator()
        .items(&refs(&items(app, &PARAGRAPH)?))
        .separator()
        .items(&refs(&items(app, &REVIEW)?))
        .build()
}

pub fn insert_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    SubmenuBuilder::new(app, "Insert")
        .items(&refs(&items(app, &INSERT)?))
        .separator()
        .items(&refs(&items(app, &EXPORT)?))
        .build()
}

/// true for ids of the writer's menu items, which go to the focused window.
pub fn is_writer_command(id: &str) -> bool {
    [&TEXT[..], &STYLES, &PARAGRAPH, &REVIEW, &INSERT, &EXPORT]
        .iter()
        .flat_map(|list| list.iter())
        .any(|entry| entry.id == id)
}
