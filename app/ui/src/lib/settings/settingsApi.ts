// every call the Settings window makes into the shell (see
// app/shell/src/settings_commands.rs). settings that live elsewhere are
// reached through their own modules: the writer's typing switches and the
// Google Drive client (writer/writerApi.ts), the name on comments.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog } from "@tauri-apps/plugin-dialog";
import { fromStored, type AppSettings } from "./settingsModel";

export interface SettingsView {
  settings: AppSettings;
  recoveryFolder: string;
  recoveryCopies: number;
  defaultAppsExplanation: string;
  platform: "windows" | "macos";
}

export async function loadSettings(): Promise<SettingsView> {
  const view = await invoke<SettingsView>("settings_get");
  return { ...view, settings: fromStored(view.settings) };
}

export async function saveSettings(settings: AppSettings): Promise<AppSettings> {
  return fromStored(await invoke("settings_set", { value: settings }));
}

/** just the stored choices, for screens that only read them (the Export dialog, the viewer). */
export async function currentSettings(): Promise<AppSettings> {
  return fromStored(await invoke("settings_get").then((view) => (view as SettingsView).settings).catch(() => ({})));
}

export function clearRecoveryCopies(): Promise<number> {
  return invoke("settings_clear_recovery");
}

export function clearRecents(): Promise<void> {
  return invoke("settings_clear_recents");
}

export function openSettingsWindow(): Promise<void> {
  return invoke("open_settings_window");
}

export function openDefaultAppsSettings(): Promise<void> {
  return invoke("open_default_apps_settings");
}

export function authorName(): Promise<string> {
  return invoke<string>("annotation_author").catch(() => "");
}

export function setAuthorName(name: string): Promise<string> {
  return invoke("set_annotation_author", { name });
}

export async function chooseFile(title: string, extensions?: string[]): Promise<string | null> {
  const chosen = await showOpenDialog({ title, multiple: false, filters: extensions ? [{ name: title, extensions }] : undefined });
  return typeof chosen === "string" ? chosen : null;
}

/** the label of the one Settings window (settings_commands.rs). */
export const SETTINGS_WINDOW = "window-settings";
