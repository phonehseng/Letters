// every call from Search All Documents into the shell, typed.
// the index lives in the app's data folder and never leaves the computer.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog } from "@tauri-apps/plugin-dialog";

export type DocKind = "pdf" | "word" | "text";

export interface Filters {
  /** empty means every kind */
  kinds: DocKind[];
  folder?: string | null;
  /** milliseconds since 1970 */
  modifiedAfter?: number | null;
  modifiedBefore?: number | null;
  /** "search this document" */
  document?: string | null;
  titlesOnly: boolean;
}

export interface SearchRequest {
  query: string;
  filters: Filters;
  maxDocuments: number;
  passagesPerDocument: number;
  now?: number;
}

export type DocNote = { kind: "needsTextRecognition"; pages: number } | { kind: "locked" } | { kind: "unreadable"; reason: string };

export interface PassageHit {
  /** the page (from 0) of a pdf, the block of a word document, the paragraph of a text file */
  number: number;
  /** the page it is on, from 0; estimated for word documents */
  page: number;
  heading: string;
  snippet: string;
  /** [start, end) of each highlighted word in the snippet */
  highlights: [number, number][];
  score: number;
}

export interface DocumentHit {
  path: string;
  title: string;
  kind: DocKind;
  modified: number;
  pages: number;
  notes: DocNote[];
  matchingPassages: number;
  passages: PassageHit[];
  score: number;
}

export interface SearchResults {
  documents: DocumentHit[];
  totalDocuments: number;
  totalPassages: number;
  corrections: { typed: string; used: string }[];
  micros: number;
}

export type JobPhase = "starting" | "scanning" | "indexing" | "idle" | "paused";

export interface JobStatus {
  phase: JobPhase;
  done: number;
  total: number;
  current: string | null;
  documents: number;
  passages: number;
  bytesOnDisk: number;
  locked: number;
  needsTextRecognition: number;
  unreadable: number;
  lastError: string | null;
  lastFinished: number | null;
  recovered: boolean;
}

export interface LibrarySettings {
  libraryFolders: string[];
  searchedPinnedFolders: string[];
  excludedFolders: string[];
  includeRecents: boolean;
  paused: boolean;
}

export interface LibraryOverview {
  settings: LibrarySettings;
  status: JobStatus;
  indexFolder: string;
  pinnedFolders: string[];
}

export type LibraryChange = "addFolder" | "removeFolder" | "excludeFolder" | "includeFolder" | "searchPinned" | "skipPinned" | "includeRecents" | "skipRecents";

export function searchLibrary(request: SearchRequest): Promise<SearchResults> {
  return invoke("library_search", { request: { ...request, now: request.now ?? Date.now() } });
}

export function libraryOverview(): Promise<LibraryOverview> {
  return invoke("library_overview");
}

export function refreshLibrary(): Promise<void> {
  return invoke("library_refresh");
}

export function changeLibrary(action: LibraryChange, path?: string): Promise<LibraryOverview> {
  return invoke("library_change", { action, path: path ?? null });
}

export function controlIndex(action: "pause" | "resume" | "rebuild" | "delete"): Promise<LibraryOverview> {
  return invoke("library_control", { action });
}

/** the folder picker for "Add Folder…". */
export async function chooseFolder(): Promise<string | null> {
  const chosen = await showOpenDialog({ directory: true, multiple: false, title: "Add a Folder to Search" });
  return typeof chosen === "string" ? chosen : null;
}
