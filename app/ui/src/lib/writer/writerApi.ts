// every call from the writer into the desktop shell (see
// app/shell/src/writer_commands.rs), so the writer's components never need
// to know command names.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog, save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { DocumentModel } from "./model";
import type { TypingSettings } from "./typing";

export interface WriterOpened {
  id: number;
  name: string;
  path: string;
  /** the format Save writes: "docx" or "odt" */
  kind: "docx" | "odt";
  model: DocumentModel;
  mediaIds: string[];
  /** a recovery copy a run that ended without saving left behind */
  recovery: { savedAt: number } | null;
}

export interface WriterSaved {
  name: string;
  path: string;
  kind: "docx" | "odt";
}

export interface WriterSettings {
  typing: TypingSettings;
  spellcheck: boolean;
  /** "free" or "strict" (Strict Word mode) */
  mode?: string;
  /** the google oauth client for direct drive uploads (Settings) */
  driveClientId?: string;
  driveClientSecret?: string;
}

export interface ExportedPdf {
  path: string;
  method: "word" | "libreOffice";
  methodLabel: string;
}

/** the editor's model without what the shell keeps itself (it never sent carry or picture bytes). */
function forShell(model: DocumentModel): DocumentModel {
  const copy: DocumentModel = { ...model };
  delete (copy as Record<string, unknown>).carry;
  delete (copy as Record<string, unknown>).media;
  return copy;
}

export function openWriterDocument(path: string, recovered = false): Promise<WriterOpened> {
  return invoke("writer_open", { path, recovered });
}

export function closeWriterDocument(id: number): Promise<void> {
  return invoke("writer_close", { id });
}

export function saveWriterDocument(id: number, model: DocumentModel, path: string | null): Promise<WriterSaved> {
  return invoke("writer_save", { id, model: forShell(model), path });
}

/** what saving as .odt would lose, in a sentence, or null. */
export function odtLosses(id: number, model: DocumentModel): Promise<string | null> {
  return invoke("writer_odt_losses", { id, model: forShell(model) });
}

export function autosaveWriterDocument(id: number, model: DocumentModel): Promise<void> {
  return invoke("writer_autosave", { id, model: forShell(model) });
}

export function discardRecovery(id: number): Promise<void> {
  return invoke("writer_discard_recovery", { id });
}

/** a picture's `data:` address, or null if the document has no such picture. */
export function mediaUrl(id: number, mediaId: string): Promise<string | null> {
  return invoke("writer_media", { id, mediaId });
}

export function addPicture(id: number, path: string): Promise<{ mediaId: string; url: string }> {
  return invoke("writer_add_picture", { id, path });
}

export function applyPreset(model: DocumentModel, preset: "mla" | "apa", details: Record<string, unknown>): Promise<DocumentModel> {
  return invoke("writer_apply_preset", { model: forShell(model), preset, details });
}

export interface RulerSentences {
  description: string[];
}

export function describeRuler(model: DocumentModel, blockIndex: number): Promise<RulerSentences> {
  return invoke("writer_describe_ruler", { model: forShell(model), blockIndex });
}

export function exportPdf(id: number, model: DocumentModel, path: string): Promise<{ exported: ExportedPdf | null; noConverter: boolean }> {
  return invoke("writer_export_pdf", { id, model: forShell(model), path });
}

export function writerSettings(): Promise<WriterSettings> {
  return invoke("writer_settings");
}

export function saveWriterSettings(settings: WriterSettings): Promise<WriterSettings> {
  return invoke("writer_set_settings", { settings });
}

/** the name on new comments and tracked changes (the same one the pdf comments use). */
export function authorName(): Promise<string> {
  return invoke<string>("annotation_author").catch(() => "");
}

export async function chooseSaveLocationFor(suggested: string, kind: "docx" | "odt" | "pdf"): Promise<string | null> {
  const filters =
    kind === "pdf"
      ? [{ name: "PDF documents", extensions: ["pdf"] }]
      : [
          { name: "Word document", extensions: ["docx"] },
          { name: "OpenDocument text", extensions: ["odt"] },
        ];
  if (kind === "odt") filters.reverse();
  const chosen = await showSaveDialog({ defaultPath: suggested, filters });
  return typeof chosen === "string" ? chosen : null;
}

// --- word's layout, copies, printing (app/shell/src/drive_commands.rs)

export function wordAvailable(): Promise<boolean> {
  return invoke<boolean>("writer_word_available").catch(() => false);
}

/** where Word breaks the pages: the page each top-level block starts on (empty when it couldn't be matched). */
export function wordLayout(id: number, model: DocumentModel): Promise<{ blockPages: number[]; pageCount: number }> {
  return invoke("writer_word_layout", { id, model: forShell(model) });
}

/** writes a copy as .docx, .odt, .html or .txt; the document keeps its own file. */
export function exportCopy(id: number, model: DocumentModel, path: string): Promise<string> {
  return invoke("writer_export_copy", { id, model: forShell(model), path });
}

/** a print-ready pdf through Word or LibreOffice, in the temp folder; fails when neither is installed. */
export function printReadyPdf(id: number, model: DocumentModel): Promise<string> {
  return invoke("writer_print_pdf", { id, model: forShell(model) });
}

/** a picture from the clipboard or a dropped file, as bytes. */
export function addPictureData(id: number, name: string, base64: string): Promise<{ mediaId: string; url: string }> {
  return invoke("writer_add_picture_data", { id, name, data: base64 });
}

// --- google drive

export interface DriveStatus {
  folders: string[];
  canUpload: boolean;
  signedIn: boolean;
}

export function driveStatus(): Promise<DriveStatus> {
  return invoke("drive_status");
}

export function driveSignIn(): Promise<void> {
  return invoke("drive_sign_in");
}

export function driveSignOut(): Promise<void> {
  return invoke("drive_sign_out");
}

export function driveUpload(id: number, model: DocumentModel, name: string, format: "docx" | "googleDoc" | "pdf", folder: string | null): Promise<{ id: string; name: string; webViewLink?: string }> {
  return invoke("drive_upload", { id, model: forShell(model), name, format, folder });
}

export function openLink(url: string): Promise<void> {
  return invoke("open_link", { url });
}

/** the window's Open… panel: pdfs and the documents the writer opens. */
export async function chooseFileToOpen(): Promise<string | null> {
  const chosen = await showOpenDialog({
    multiple: false,
    filters: [
      { name: "PDFs and documents", extensions: ["pdf", "docx", "odt"] },
      { name: "PDF documents", extensions: ["pdf"] },
      { name: "Word and OpenDocument", extensions: ["docx", "odt"] },
    ],
  });
  return typeof chosen === "string" ? chosen : null;
}

/** the save panel with a list of formats, the first shown first. */
export async function chooseSaveLocationWithFilters(suggested: string, filters: { name: string; extensions: string[] }[]): Promise<string | null> {
  const chosen = await showSaveDialog({ defaultPath: suggested, filters });
  return typeof chosen === "string" ? chosen : null;
}

export async function choosePicture(): Promise<string | null> {
  const chosen = await showOpenDialog({ multiple: false, filters: [{ name: "Pictures", extensions: ["png", "jpg", "jpeg", "gif", "bmp"] }] });
  return typeof chosen === "string" ? chosen : null;
}

/** "essay.docx" in the same folder with another extension: "essay.pdf". */
export function withExtension(path: string, extension: string): string {
  return path.replace(/\.[^.\\/]+$/, "") + `.${extension}`;
}
