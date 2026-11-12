// every call into the shell for reading and writing long documents, typed.
// all of it is kept on this computer; only saveBookmarksIntoPdf changes a
// document, and that through an edit that waits for Save.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";

export interface Bookmark {
  id: string;
  name: string;
  /** counted from 0 */
  page: number;
  block: number | null;
  created: number;
}

export interface DocumentReading {
  lastPage: number | null;
  lastFraction: number;
  lastBlock: number | null;
  updated: number;
  bookmarks: Bookmark[];
  secondsPerPage: number | null;
  wordGoal: number | null;
  sessionGoal: number | null;
}

export interface WritingLog {
  days: Record<string, number>;
  dailyGoal: number;
}

export interface Version {
  id: string;
  name: string;
  saved: number;
  words: number;
  bytes: number;
  file: string;
}

export const readingDocument = (path: string): Promise<DocumentReading> => invoke("reading_document", { path });

export const setReadingPosition = (path: string, page: number, fraction: number, block: number | null = null): Promise<void> => invoke("reading_set_position", { path, page, fraction, block });

export const clearReadingPosition = (path: string): Promise<void> => invoke("reading_clear_position", { path });

export const addBookmark = (path: string, name: string, page: number, block: number | null = null): Promise<Bookmark> => invoke("reading_add_bookmark", { path, name, page, block });

export const renameBookmark = (path: string, id: string, name: string): Promise<boolean> => invoke("reading_rename_bookmark", { path, id, name });

export const removeBookmark = (path: string, id: string): Promise<boolean> => invoke("reading_remove_bookmark", { path, id });

export const recordPageTime = (path: string, seconds: number): Promise<number | null> => invoke("reading_record_page_time", { path, seconds });

export const pageLabels = (id: number): Promise<string[]> => invoke("reading_page_labels", { id });

export const saveBookmarksIntoPdf = (id: number, bookmarks: { name: string; page: number }[]): Promise<DocumentState> => invoke("reading_save_bookmarks_to_pdf", { id, bookmarks });

export const writingLog = (): Promise<WritingLog> => invoke("writing_log");

export const recordWords = (day: string, words: number): Promise<WritingLog> => invoke("writing_record_words", { day, words });

export const setDailyGoal = (words: number): Promise<WritingLog> => invoke("writing_set_daily_goal", { words });

export const setGoals = (path: string, wordGoal: number | null, sessionGoal: number | null): Promise<DocumentReading> => invoke("writing_set_goals", { path, wordGoal, sessionGoal });

export const listVersions = (path: string): Promise<Version[]> => invoke("versions_list", { path });

export const saveVersion = (path: string, name: string, words: number): Promise<Version> => invoke("versions_save", { path, name, words });

export const restoreVersion = (path: string, id: string, words: number): Promise<Version> => invoke("versions_restore", { path, id, words });
