// every call the Compare screen makes into the shell, typed, and the shape
// of a comparison as the engine (core/compare) sends it. pages are counted
// from 0; rectangles are in points from the page's top left, y down.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog, save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { PageSize } from "../layout";

export type Category = "text" | "formatting" | "layout" | "image" | "page";

export type ChangeKind =
  | "inserted"
  | "deleted"
  | "replaced"
  | "moved"
  | "restyled"
  | "shifted"
  | "reflowed"
  | "resized"
  | "redrawn"
  | "pageAdded"
  | "pageRemoved"
  | "pageMoved";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Change {
  /** unique in the result, so both sides of a change can be linked */
  id: number;
  kind: ChangeKind;
  category: Category;
  /** the old page the left rectangles are on (a moved passage may come from another page) */
  leftPage: number | null;
  rightPage: number | null;
  leftRects: Rect[];
  rightRects: Rect[];
  oldText: string | null;
  newText: string | null;
  /** one plain sentence from the engine */
  detail: string;
}

export interface ScrollPoint {
  leftY: number;
  rightY: number;
}

export interface PagePair {
  leftPage: number | null;
  rightPage: number | null;
  leftSize: PageSize | null;
  rightSize: PageSize | null;
  similarity: number;
  changes: Change[];
  scrollMap: ScrollPoint[];
  offset: { dx: number; dy: number };
  notes: string[];
}

export interface Summary {
  text: number;
  formatting: number;
  layout: number;
  image: number;
  page: number;
  total: number;
  /** differences only in spacing or hyphenation, left out unless asked for */
  ignored: number;
  pagePairs: number;
  pagePairsWithChanges: number;
}

export interface ComparisonResult {
  leftPageCount: number;
  rightPageCount: number;
  pagePairs: PagePair[];
  summary: Summary;
  notes: string[];
}

export interface CompareOptions {
  ignoreSpacing: boolean;
  compareFormatting: boolean;
  compareLayout: boolean;
  comparePictures: boolean;
}

export const DEFAULT_OPTIONS: CompareOptions = {
  ignoreSpacing: true,
  compareFormatting: true,
  compareLayout: true,
  comparePictures: true,
};

/** one side to compare: a file on disk, or a document open in this window. */
export type CompareInput = { kind: "file"; path: string } | { kind: "open"; id: number };

export interface CompareSide {
  name: string;
  documentId: number;
  pages: PageSize[];
}

export interface CompareStarted {
  jobId: number;
  old: CompareSide;
  new: CompareSide;
}

export type Stage = "readingOld" | "readingNew" | "comparingPages";

export type JobStatus =
  | { state: "running"; stage: Stage | null; done: number; total: number }
  | { state: "finished"; summary: Summary }
  | { state: "failed"; message: string }
  | { state: "cancelled" };

export interface PairPictures {
  old: string;
  new: string;
  overlay: string;
}

export type ReportFormat = "pdf" | "html" | "text";

export interface ReportOptions {
  title?: string;
  oldName: string;
  newName: string;
  madeOn?: string;
  thumbnails?: boolean;
  language?: string;
}

export function startComparison(old: CompareInput, next: CompareInput, options: CompareOptions): Promise<CompareStarted> {
  return invoke("compare_start", { old, new: next, options });
}

export function comparisonStatus(jobId: number): Promise<JobStatus> {
  return invoke("compare_status", { jobId });
}

export function comparisonResult(jobId: number): Promise<ComparisonResult> {
  return invoke("compare_result", { jobId });
}

export function cancelComparison(jobId: number): Promise<void> {
  return invoke("compare_cancel", { jobId });
}

/** closes the comparison: both documents leave the engine. */
export function closeComparison(jobId: number): Promise<void> {
  return invoke("compare_close", { jobId });
}

export function pairPictures(jobId: number, pair: number, scale: number): Promise<PairPictures> {
  return invoke("compare_pair_pictures", { jobId, pair, scale });
}

export function saveReport(jobId: number, path: string, format: ReportFormat, options: ReportOptions): Promise<string> {
  return invoke("compare_save_report", { jobId, path, format, options });
}

/** the system's file picker for one pdf to compare. */
export async function choosePdf(): Promise<string | null> {
  const chosen = await showOpenDialog({ multiple: false, filters: [{ name: "PDF documents", extensions: ["pdf"] }] });
  return typeof chosen === "string" ? chosen : null;
}

const REPORT_FILTERS: Record<ReportFormat, { name: string; extensions: string[] }> = {
  pdf: { name: "PDF documents", extensions: ["pdf"] },
  html: { name: "Web pages", extensions: ["html"] },
  text: { name: "Plain text", extensions: ["txt"] },
};

/** the save panel for a report, starting at `suggestedPath`. */
export async function chooseReportLocation(suggestedPath: string, format: ReportFormat): Promise<string | null> {
  const chosen = await showSaveDialog({ defaultPath: suggestedPath, filters: [REPORT_FILTERS[format]] });
  return typeof chosen === "string" ? chosen : null;
}
