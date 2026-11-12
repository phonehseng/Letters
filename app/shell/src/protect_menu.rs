//! the Protect menu in the macos menu bar: redaction, hidden information
//! and passwords.
//!
//! it works like every other menu (see menu.rs): picking an item sends
//! its id to the focused window as a `menu-command`, and the ui does what
//! the Protect tab's own buttons do. items with an ellipsis open the tab
//! at that tool, because each of them asks something before it acts.
//! marking has keys (control command R and B, ctrl alt R and B on
//! windows), so redaction can be done from the keyboard.

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

const MARK: [Item; 3] = [
    item(
        "mark-redaction",
        "Mark Text for Redaction",
        Some("Ctrl+Cmd+R"),
    ),
    item(
        "mark-redaction-area",
        "Mark Area for Redaction",
        Some("Ctrl+Cmd+B"),
    ),
    item("find-and-mark", "Find and Mark…", None),
];

const APPLY: [Item; 2] = [
    item("review-redactions", "Review Redactions…", None),
    item("apply-redactions", "Apply Redactions…", None),
];

const FILE: [Item; 2] = [
    item(
        "remove-hidden-information",
        "Remove Hidden Information…",
        None,
    ),
    item("password-settings", "Passwords and Permissions…", None),
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
    items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect()
}

pub fn protect_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    SubmenuBuilder::new(app, "Protect")
        .items(&refs(&items(app, &MARK)?))
        .separator()
        .items(&refs(&items(app, &APPLY)?))
        .separator()
        .items(&refs(&items(app, &FILE)?))
        .build()
}

/// true for ids of the Protect menu's items, which go to the focused window.
pub fn is_protect_command(id: &str) -> bool {
    [&MARK[..], &APPLY, &FILE]
        .iter()
        .flat_map(|list| list.iter())
        .any(|entry| entry.id == id)
}
