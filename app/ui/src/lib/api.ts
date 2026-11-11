// every call from the ui into the desktop shell goes through this file,
// so the rest of the ui never needs to know command names.

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog } from "@tauri-apps/plugin-dialog";
import type { PageSize } from "./layout";

export interface OpenedDocument {
  id: number;
  name: string;
  path: string;
  pages: PageSize[];
}

export interface RecentFile {
  path: string;
  name: string;
  pageCount: number;
  /** seconds since 1970 */
  openedAt: number;
  thumbnail: string;
}

/** `kind` tells the ui how to react; `message` is safe to show as it is. */
export interface CommandError {
  kind: "password-required" | "engine-unavailable" | "failed";
  message: string;
}

export function asCommandError(error: unknown): CommandError {
  if (typeof error === "object" && error !== null && "kind" in error && "message" in error) {
    return error as CommandError;
  }
  return { kind: "failed", message: String(error) };
}

export function openDocument(path: string, password?: string): Promise<OpenedDocument> {
  return invoke("open_document", { path, password: password ?? null });
}

export function closeDocument(id: number): Promise<void> {
  return invoke("close_document", { id });
}

export function takeStartupFile(): Promise<string | null> {
  return invoke("take_startup_file");
}

export function newWindow(path?: string): Promise<void> {
  return invoke("new_window", { path: path ?? null });
}

export function recentFiles(): Promise<RecentFile[]> {
  return invoke("recent_files");
}

export function forgetRecentFile(path: string): Promise<void> {
  return invoke("forget_recent_file", { path });
}

/** shows the system's file picker and returns the chosen pdf, if any. */
export async function chooseFile(): Promise<string | null> {
  const chosen = await showOpenDialog({
    multiple: false,
    filters: [{ name: "PDF documents", extensions: ["pdf"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}

// the start of every tile address. it differs by system
// (http://tile.localhost/ on windows, tile://localhost/ elsewhere),
// so we ask the shell's helper once instead of guessing.
const tileBase = convertFileSrc("", "tile");

export function tileUrl(
  documentId: number,
  page: number,
  scale: number,
  column: number,
  row: number,
): string {
  return `${tileBase}${documentId}/${page}/${scale}/${column}/${row}`;
}

export function thumbnailUrl(file: RecentFile): string {
  // opened-at is added so a reopened file shows its fresh thumbnail
  return `${tileBase}thumbnail/${file.thumbnail}?v=${file.openedAt}`;
}
