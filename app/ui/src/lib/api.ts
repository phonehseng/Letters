// every call from the ui into the desktop shell goes through this file,
// so the rest of the ui never needs to know command names.

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
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

/** shows the system's folder picker and returns the chosen folder, if any. */
export async function chooseFolder(): Promise<string | null> {
  const chosen = await showOpenDialog({ directory: true, multiple: false });
  return typeof chosen === "string" ? chosen : null;
}

/** one entry of a document's table of contents (its bookmarks). */
export interface OutlineItem {
  title: string;
  /** counted from 0 */
  page: number;
  /** 0 for a top level entry, 1 for an entry inside it, and so on */
  level: number;
}

/** the outline in reading order, or an empty list when the document has none. */
export function documentOutline(id: number): Promise<OutlineItem[]> {
  return invoke("document_outline", { id });
}

/** a rectangle in points, from the page's top left corner, y going down. */
export interface PageRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SearchMatch {
  /** counted from 0 */
  page: number;
  /** one rectangle per line the match covers */
  rects: PageRect[];
  snippet: string;
}

export interface SearchResult {
  matches: SearchMatch[];
  /** true when the engine stopped at its limit and there are more */
  truncated: boolean;
}

export function searchText(id: number, query: string): Promise<SearchResult> {
  return invoke("search_text", { id, query });
}

export interface FolderFile {
  path: string;
  name: string;
  /** seconds since 1970 */
  modifiedAt: number;
}

export interface PinnedFolder {
  path: string;
  name: string;
  files: FolderFile[];
}

export function pinnedFolders(): Promise<PinnedFolder[]> {
  return invoke("pinned_folders");
}

export function pinFolder(path: string): Promise<PinnedFolder[]> {
  return invoke("pin_folder", { path });
}

export function unpinFolder(path: string): Promise<PinnedFolder[]> {
  return invoke("unpin_folder", { path });
}

/**
 * calls `onCommand` whenever this window's menu bar sends a command.
 * only macOS has the menu; elsewhere this simply never fires.
 *
 * it listens on this window only: the plain `listen` hears events meant
 * for every window, and a menu command belongs to the window in front.
 * returns a function that stops listening.
 */
export function listenForMenuCommands(onCommand: (command: string) => void): () => void {
  const stop = getCurrentWebviewWindow().listen<string>("menu-command", (event) => onCommand(event.payload));
  return () => {
    stop.then((unlisten) => unlisten());
  };
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
