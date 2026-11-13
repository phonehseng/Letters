// every call the Export dialog and file opening make into the shell (see
// app/shell/src/export_commands.rs and associations.rs).

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open as showOpenDialog, save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { WriterOpened } from "../writer/writerApi";
import { CONVERTIBLE_EXTENSIONS, type ConvertChoices, type ConvertedFile, type ConvertFormat } from "./convertFile";
import type { ExportCapabilities, ExportOutcome, ExportRequest, PageSelection } from "./exportChoices";

export function exportCapabilities(): Promise<ExportCapabilities> {
  return invoke("export_capabilities_list");
}

export interface ExportJob {
  request: ExportRequest;
  pages: PageSelection;
  destination: string;
}

/** runs an export; progress arrives through `listenForExportProgress`. */
export function exportDocument(id: number, exportId: number, job: ExportJob): Promise<ExportOutcome> {
  return invoke("export_document", { id, exportId, job });
}

export function cancelExport(exportId: number): Promise<void> {
  return invoke("export_cancel", { exportId });
}

export interface ExportProgressEvent {
  exportId: number;
  fraction: number;
  page: number | null;
}

/** progress of this window's exports. returns a function that stops listening. */
export function listenForExportProgress(onProgress: (event: ExportProgressEvent) => void): () => void {
  const stop = getCurrentWebviewWindow().listen<ExportProgressEvent>("export-progress", (event) => onProgress(event.payload));
  return () => {
    stop.then((unlisten) => unlisten());
  };
}

export async function chooseExportLocation(suggested: string, filter: { name: string; extensions: string[] }): Promise<string | null> {
  const chosen = await showSaveDialog({ defaultPath: suggested, filters: [filter] });
  return typeof chosen === "string" ? chosen : null;
}

export interface ConvertedToPdf {
  pdf: string;
  program: string;
  summary: string;
}

/** "Convert to PDF": an office file made into a pdf at `destination`. */
export function convertOfficeToPdf(path: string, destination: string): Promise<ConvertedToPdf> {
  return invoke("convert_office_to_pdf", { path, destination });
}

/** a spreadsheet, presentation or rtf as a pdf in the temp folder, for the viewer. */
export function showOfficeAsPdf(path: string): Promise<ConvertedToPdf> {
  return invoke("show_office_as_pdf", { path });
}

/** "Open PDF as Document…": a new window that shows the pdf rebuilt as an editable document. */
export function openPdfAsDocumentWindow(path: string): Promise<void> {
  return invoke("open_pdf_as_document_window", { path });
}

/** true once, in the window opened for that pdf by `openPdfAsDocumentWindow`. */
export function takeOpenAsDocument(path: string): Promise<boolean> {
  return invoke<boolean>("take_open_as_document", { path }).catch(() => false);
}

/** the pdf rebuilt as an editable document in this window, unsaved. */
export function openPdfFileAsDocument(path: string): Promise<WriterOpened> {
  return invoke("open_pdf_file_as_document", { path });
}

export type OpenRoute = "viewer" | "writer" | "picture" | "sheets" | "slides" | "officeAsPdf" | "unsupported";

/** where a file opens, by its kind (shell-core/src/associations). */
export function openRoute(path: string): Promise<OpenRoute> {
  return invoke("open_route", { path });
}

// --- Convert File… (app/shell-core/src/export/convert_file.rs)

/** the types a file can become on this computer. */
export function convertFileChoices(path: string): Promise<ConvertChoices> {
  return invoke("convert_file_choices", { path });
}

/** saves `path` as `format` at `destination`; the original is only read. */
export function convertFile(path: string, format: ConvertFormat, destination: string): Promise<ConvertedFile> {
  return invoke("convert_file", { path, format, destination });
}

/** the file picker for Convert File…: every kind Letters converts or sends on. */
export async function chooseFileToConvert(): Promise<string | null> {
  const chosen = await showOpenDialog({
    multiple: false,
    filters: [{ name: "Files Letters converts", extensions: CONVERTIBLE_EXTENSIONS }],
  });
  return typeof chosen === "string" ? chosen : null;
}

/** where the converted copy goes, suggested beside the original. */
export async function chooseConvertDestination(suggested: string, extension: string, label: string): Promise<string | null> {
  const chosen = await showSaveDialog({ defaultPath: suggested, filters: [{ name: label, extensions: [extension] }] });
  return typeof chosen === "string" ? chosen : null;
}
