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
  /** the rich text or old Word file this was converted from; Save writes `path`, a .docx beside it */
  convertedFrom?: string | null;
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
  /** Free mode lays out pages with Letters' own engine instead of estimating them */
  layoutEngine?: boolean;
  /** the google oauth client for direct drive uploads (Settings) */
  driveClientId?: string;
  driveClientSecret?: string;
}

export interface PrintPageSetup {
  section: number;
  /** "Letter", "A4"... or "Custom" */
  paper: string;
  widthMm: number;
  heightMm: number;
  landscape: boolean;
  /** top, right, bottom, left */
  marginsInches: [number, number, number, number];
}

export interface ExportedPdf {
  path: string;
  method: "word" | "libreOffice";
  methodLabel: string;
  /** made by Word, so it matches Word page for page */
  exact?: boolean;
  /** a plain sentence when it isn't exact */
  note?: string | null;
  /** each section's paper and margins */
  pages?: PrintPageSetup[];
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

/** the document's file moved: Save writes to `to` from now on. */
export function followMove(id: number, to: string): Promise<WriterSaved> {
  return invoke("writer_follow_move", { id, to });
}

export function closeWriterDocument(id: number): Promise<void> {
  return invoke("writer_close", { id });
}

/** `replaceChanged`: the person chose to write over a change made to the file elsewhere. */
export function saveWriterDocument(id: number, model: DocumentModel, path: string | null, replaceChanged = false): Promise<WriterSaved> {
  return invoke("writer_save", { id, model: forShell(model), path, replaceChanged });
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

/** one line as Word laid it out (shell-core/src/writer/layout.rs). */
export interface WordLine {
  page: number;
  block: number;
  /** inside a table: row, cell and block within the cell */
  cell?: [number, number, number];
  /** characters from the start of Word's paragraph to the line's start */
  offset: number;
  top: number;
  left: number;
  text: string;
}

export interface WordShape {
  name: string;
  page: number;
  left: number;
  top: number;
  width: number;
  height: number;
  relativeHorizontal: number;
  relativeVertical: number;
  text: string;
}

export interface WordLayoutAnswer {
  /** the page each top-level block starts on (empty when it couldn't be matched) */
  blockPages: number[];
  pageCount: number;
  lines: WordLine[];
  shapes: WordShape[];
  /** how long the answer took, Word's start included */
  milliseconds: number;
  wordMilliseconds: number;
}

/** Letters' own lines and pages (core/layout), in the same form as Word's. */
export function engineLayout(id: number, model: DocumentModel): Promise<WordLayoutAnswer> {
  return invoke("writer_engine_layout", { id, model: forShell(model) });
}

/** Word's own lines and pages for the document as it is now (cached by the shell). */
export function wordLayout(id: number, model: DocumentModel): Promise<WordLayoutAnswer> {
  return invoke("writer_word_layout", { id, model: forShell(model) });
}

// --- objects on the page, strict word mode, the clipboard and formats

/** an object by where it lives in the model (core/office floating::ObjectRef). */
export type ObjectRef = { type: "textBox"; block: number } | { type: "image"; block: number; inline: number };

export type ObjectOp =
  | { op: "splitIntoTextBoxes"; block: number; offset: number }
  | { op: "textBoxFromBlocks"; start: number; end: number }
  | { op: "dissolveTextBox"; block: number; at: number }
  | { op: "floatImage"; block: number; inline: number; hint: [number, number] | null; wrap: string }
  | { op: "inlineImage"; block: number; inline: number; to: [number, number] | null }
  | { op: "bringToFront"; object: ObjectRef }
  | { op: "sendToBack"; object: ObjectRef }
  | { op: "align"; objects: ObjectRef[]; edge: "left" | "center" | "right" | "top" | "middle" | "bottom" }
  | { op: "distribute"; objects: ObjectRef[]; horizontal: boolean }
  | { op: "applyFixes"; only?: number };

export interface ObjectAnswer {
  model: DocumentModel;
  changed: boolean;
  /** the blocks the command made or moved: start, end */
  blocks: [number, number] | null;
  /** a picture's new place after it went back in the line: block, inline */
  image: [number, number] | null;
  refused: string | null;
}

export function objectCommand(id: number, model: DocumentModel, op: ObjectOp): Promise<ObjectAnswer> {
  return invoke("writer_objects", { id, model: forShell(model), op });
}

export interface EngineFeature {
  key: string;
  label: string;
  support: "both" | "wordExact" | "odtExact" | "approximate";
  note: string;
}

export interface CompatibilityIssue {
  code: string;
  message: string;
  location?: { block: number; inline?: number };
  fix?: { type: string; [field: string]: unknown };
}

/** the engine's feature table and what Word would show differently. */
export function compatibility(id: number, model: DocumentModel): Promise<{ features: EngineFeature[]; report: { issues: CompatibilityIssue[] } }> {
  return invoke("writer_compatibility", { id, model: forShell(model) });
}

export interface Copied {
  html: string;
  text: string;
  /** a lone picture: its bytes (base64) and content type */
  image: [string, string] | null;
}

/** the clipboard forms of some blocks (the copied part as the editor has it). */
export function copyBlocks(id: number, model: DocumentModel, blocks: unknown[]): Promise<Copied> {
  return invoke("writer_copy", { id, model: forShell(model), blocks });
}

export interface PastedPicture {
  inline: Record<string, unknown>;
  mediaId: string;
  url: string;
  note: string | null;
}

export function pastePicture(id: number, model: DocumentModel, base64: string, description: string | null): Promise<PastedPicture> {
  return invoke("writer_paste_picture", { id, model: forShell(model), data: base64, description });
}

export interface PastedHtml {
  blocks: unknown[];
  numbering: { definitions?: unknown[]; instances?: unknown[] };
  pictures: [string, string][];
}

export function pasteHtml(id: number, html: string): Promise<PastedHtml> {
  return invoke("writer_paste_html", { id, html });
}

export interface FormatCapability {
  format: "docx" | "odt" | "rtf" | "html" | "plainText" | "markdown" | "doc";
  label: string;
  extension: string;
  canOpen: boolean;
  canSave: boolean;
  needsConverter: boolean;
  summary: string;
}

/** the engine's format table, and which converters this computer has. */
export function saveFormats(): Promise<{ formats: FormatCapability[]; word: boolean; libreOffice: boolean }> {
  return invoke("writer_formats");
}

/** writes a copy in any of the engine's formats, chosen by the name; the document keeps its own file. */
export function exportCopy(id: number, model: DocumentModel, path: string): Promise<string> {
  return invoke("writer_export_copy", { id, model: forShell(model), path });
}

/** a print-ready pdf through Word (exact) or LibreOffice (close, with a note), in the temp folder; fails when neither is installed. */
export function printReadyPdf(id: number, model: DocumentModel): Promise<ExportedPdf> {
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
      { name: "PDFs and documents", extensions: ["pdf", "docx", "odt", "rtf", "doc"] },
      { name: "PDF documents", extensions: ["pdf"] },
      { name: "Word, OpenDocument and Rich Text", extensions: ["docx", "odt", "rtf", "doc"] },
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
