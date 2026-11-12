// every call from the code editor into the desktop shell (see
// app/shell/src/code_commands.rs), so components never need command names.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog, save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { CodeExportFormat } from "./codeExport";
import type { FoundCode } from "./codeBlocks";
import type { OutputFormat } from "./diagnostics";
import { CODE_EXTENSIONS, guessLanguage, languageForPath, type LanguageId } from "./languages";
import type { TextFormat } from "./textFormat";

export interface OpenedCode {
  path: string;
  name: string;
  text: string;
  format: TextFormat;
  bytes: number;
  readOnly: boolean;
  fullFeatures: boolean;
  recovery: { savedAt: number; text: string } | null;
  /** the file as it was read; handed back with each save (see saveCodeFile) */
  diskStamp: DiskStamp | null;
}

/** a file's size and last change, as this window last saw them. */
export interface DiskStamp {
  bytes: number;
  modifiedMs: number;
}

/**
 * what the code screen shows: a file, or an untitled block from a document.
 * an untitled block has no path, so Save asks where; `language` is the
 * guess it starts in, since its name says nothing.
 */
export type CodeDocument = Omit<OpenedCode, "path"> & { path: string | null; language?: LanguageId };

/**
 * a file for the code screen. an untitled block (handed over through a
 * temp file, see app/shell-core/src/code/untitled.rs) comes back without a
 * path, so Save asks where, and with its language guessed from the text.
 */
export async function openCodeDocument(path: string): Promise<CodeDocument> {
  const opened = await invoke<OpenedCode & { untitled: boolean }>("code_open", { path });
  if (!opened.untitled) return opened;
  return { ...opened, path: null, language: languageForPath(opened.name) ?? guessLanguage(opened.text) };
}

/** "Open Block in Code Editor": a new window with this text, untitled. */
export function openUntitledCode(text: string, extension: string): Promise<void> {
  return invoke("code_open_untitled", { text, extension });
}

/**
 * saves, unless the file changed on disk since `expected` (another window
 * or program saved it): then it refuses with a sentence suggesting Save As.
 * answers the file's new stamp, for the next save.
 */
export function saveCodeFile(
  path: string,
  text: string,
  format: TextFormat,
  expected: DiskStamp | null,
): Promise<DiskStamp | null> {
  return invoke("code_save", { path, text, format, expected });
}

export function autosaveCode(path: string, text: string): Promise<void> {
  return invoke("code_autosave", { path, text });
}

export function discardCodeRecovery(path: string): Promise<void> {
  return invoke("code_discard_recovery", { path });
}

export interface FoundTool {
  kind: OutputFormat | "vsCode";
  path: string;
  label: string;
}

export function codeTools(refresh = false): Promise<{ tools: FoundTool[]; vscode: boolean }> {
  return invoke("code_tools", { refresh });
}

export type CheckLanguage = "c" | "cpp" | "python" | "java" | "javaScript";

export interface CheckOutcome {
  checker: string | null;
  outputFormat: OutputFormat | null;
  output: string;
  fileName: string;
  timedOut: boolean;
  cancelled: boolean;
  note: string | null;
}

export function compilerCheck(language: CheckLanguage, text: string, path: string | null): Promise<CheckOutcome> {
  return invoke("code_check", { request: { language, text, path } });
}

export function cancelCompilerCheck(): Promise<void> {
  return invoke("code_cancel_check");
}

export function openInVsCode(path: string, line: number, column: number): Promise<void> {
  return invoke("code_open_in_vscode", { path, line, column });
}

export function revealInFolder(path: string): Promise<void> {
  return invoke("code_reveal", { path });
}

export interface CodeExported {
  path: string;
  files: string[];
  sentence: string;
}

export function exportAsCode(source: string, kind: "writer" | "pdf", format: CodeExportFormat, destination: string): Promise<CodeExported> {
  return invoke("code_export_document", { source, kind, format, destination });
}

export function findCodeInDocument(source: string, kind: "writer" | "pdf", pdfId: number | null): Promise<FoundCode[]> {
  return invoke("code_find_in_document", { source, kind, pdfId });
}

// the document kinds the other screens open (lib/writer/writerApi.ts lists the same)
const DOCUMENT_EXTENSIONS = ["pdf", "docx", "odt", "rtf", "doc"];

/**
 * the window's Open… panel: pdfs and documents first, as before, then code
 * and text files, then any file (text with an unknown extension opens as
 * plain text; anything else is refused with a sentence).
 */
export async function chooseAnyFileToOpen(): Promise<string | null> {
  const chosen = await showOpenDialog({
    multiple: false,
    filters: [
      { name: "PDFs, documents and code", extensions: [...DOCUMENT_EXTENSIONS, ...CODE_EXTENSIONS] },
      { name: "PDF documents", extensions: ["pdf"] },
      { name: "Word, OpenDocument and Rich Text", extensions: ["docx", "odt", "rtf", "doc"] },
      { name: "Code and text files", extensions: CODE_EXTENSIONS },
      { name: "All files", extensions: ["*"] },
    ],
  });
  return typeof chosen === "string" ? chosen : null;
}

export async function chooseCodeSaveLocation(suggested: string, extension: string, label: string): Promise<string | null> {
  const filters = extension ? [{ name: label, extensions: [extension] }] : [];
  const chosen = await showSaveDialog({ defaultPath: suggested, filters });
  return typeof chosen === "string" ? chosen : null;
}
