//! the Forms menu in the macos menu bar (Fill & Sign).
//!
//! it works like every other menu (see menu.rs): picking an item sends
//! its id to the focused window as a `menu-command`, and the ui does what
//! the Fill & Sign tab's own buttons do. items with an ellipsis open the
//! tab at that tool. Next Empty Field has a key (command option E, ctrl
//! alt E on windows), because it is how a form is filled from the keyboard.

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

const FILL: [Item; 3] = [
    item("fill-form", "Fill Form", None),
    item(
        "next-empty-field",
        "Next Empty Field",
        Some("CmdOrCtrl+Alt+E"),
    ),
    item("toggle-fields", "Show Fields", None),
];

const FIELDS: [Item; 4] = [
    item("find-fields", "Find Fields…", None),
    item("add-field", "Add Field…", None),
    item("edit-fields", "Edit Fields", None),
    item("flatten-form", "Flatten Form…", None),
];

const SIGN: [Item; 3] = [
    item("sign-with-picture", "Sign with a Picture…", None),
    item("sign-with-certificate", "Sign with a Certificate…", None),
    item("show-signatures", "Verify Signatures", None),
];

// forms round two: the whole-document checks (the ui's standards/StandardsSheet.svelte)
const CHECK: [Item; 1] = [item("prepare-and-check", "Prepare & Check…", None)];

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

pub fn forms_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    SubmenuBuilder::new(app, "Forms")
        .items(&refs(&items(app, &FILL)?))
        .separator()
        .items(&refs(&items(app, &FIELDS)?))
        .separator()
        .items(&refs(&items(app, &SIGN)?))
        .separator()
        .items(&refs(&items(app, &CHECK)?))
        .build()
}

/// true for ids of the Forms menu's items, which go to the focused window.
pub fn is_forms_command(id: &str) -> bool {
    [&FILL[..], &FIELDS, &SIGN, &CHECK]
        .iter()
        .flat_map(|list| list.iter())
        .any(|entry| entry.id == id)
}
