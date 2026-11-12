// the calls for Form Data: answers out to a file, in from a file, and
// many filled copies merged into one table. the file's kind comes from its
// name (.fdf, .xfdf, .csv, .json), as the save panel's filters set it.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog, save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { DocumentState } from "../api";

export type DataKind = "fdf" | "xfdf" | "csv" | "json";

export const DATA_KIND_LABELS: Record<DataKind, string> = {
  fdf: "FDF",
  xfdf: "XFDF",
  csv: "CSV",
  json: "JSON",
};

/** a sentence per kind, for the Explain-like hint under the choice */
export const DATA_KIND_HINTS: Record<DataKind, string> = {
  fdf: "The PDF standard’s own form data file; other PDF apps import it.",
  xfdf: "The same answers as XML.",
  csv: "A table for a spreadsheet: a row of field names and a row of answers.",
  json: "Field names and answers for programs and scripts.",
};

export interface SkippedField {
  field: string;
  reason: string;
}

export interface ImportReport {
  filled: string[];
  skipped: SkippedField[];
  /** about the file itself, such as which of a csv's rows of answers was used */
  note?: string;
}

export interface MergeResult {
  path: string;
  rows: number;
  columns: number;
  skipped: SkippedField[];
}

export function exportData(id: number, path: string): Promise<void> {
  return invoke("forms_export_data", { id, path });
}

export function importData(id: number, path: string): Promise<{ document: DocumentState; report: ImportReport }> {
  return invoke("forms_import_data", { id, path });
}

export function mergeData(paths: string[], destination: string): Promise<MergeResult> {
  return invoke("forms_merge_data", { paths, destination });
}

const DATA_FILTER = { name: "Form data", extensions: ["fdf", "xfdf", "csv", "json"] };

export async function chooseDataFile(): Promise<string | null> {
  const chosen = await showOpenDialog({ multiple: false, filters: [DATA_FILTER] });
  return typeof chosen === "string" ? chosen : null;
}

export async function chooseDataDestination(suggested: string, kind: DataKind): Promise<string | null> {
  const chosen = await showSaveDialog({ defaultPath: suggested, filters: [{ name: DATA_KIND_LABELS[kind], extensions: [kind] }] });
  return typeof chosen === "string" ? chosen : null;
}

export async function chooseFilledCopies(): Promise<string[]> {
  const chosen = await showOpenDialog({ multiple: true, filters: [{ name: "PDF documents", extensions: ["pdf"] }] });
  if (Array.isArray(chosen)) return chosen;
  return typeof chosen === "string" ? [chosen] : [];
}

/** "Essay.pdf" becomes "Essay answers.json" beside it. */
export function dataFileBeside(pdfPath: string, kind: DataKind): string {
  return pdfPath.replace(/\.pdf$/i, "") + ` answers.${kind}`;
}

/** what importing did, in one or two sentences. */
export function importSummary(report: ImportReport): string {
  const filled = report.filled.length;
  const said = `${filled} ${filled === 1 ? "answer" : "answers"} filled in.`;
  const skipped = report.skipped.length;
  const left = skipped === 0 ? "" : ` ${skipped} left out: ${report.skipped.map((each) => `${each.field} (${each.reason})`).join("; ")}`;
  return `${said}${left}${report.note ? ` ${report.note}` : ""}`;
}
