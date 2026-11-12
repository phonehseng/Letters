// every Protect call into the shell, typed: redaction, hidden
// information and passwords (app/shell/src/protect_commands.rs).
// the shapes follow core/redact, whose types are serde camelCase.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState, PageRect } from "../api";
import type { PageSize } from "../layout";

/** the boxes to redact on one page, counted from 0, in points from the visible top left. */
export interface RedactionArea {
  page: number;
  rects: PageRect[];
}

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface RedactOptions {
  fill: Rgb;
  /** a short label drawn in each box, such as "REDACTED" */
  overlayText: string | null;
  /** keep crossing drawings and clip them, instead of removing them whole (off: safer) */
  clipCrossingPaths: boolean;
}

/** what to look for. */
export type TextQuery =
  | { kind: "phrase"; text: string; matchCase: boolean }
  | { kind: "pattern"; pattern: string; matchCase: boolean }
  | { kind: "emailAddresses" }
  | { kind: "phoneNumbers" }
  | { kind: "longNumbers" };

export interface FoundText {
  page: number;
  text: string;
  /** one box per line the match covers */
  rects: PageRect[];
}

/** what one marked box would remove. */
export interface AreaPreview {
  page: number;
  rect: PageRect;
  /** the removed text, one string per line */
  texts: string[];
  images: number;
  paths: number;
  annotations: string[];
  formValues: string[];
}

export interface PageReport {
  page: number;
  glyphsRemoved: number;
  imagesRemoved: number;
  imagesRewritten: number;
  pathsRemoved: number;
  pathsCut: number;
  pathsClipped: number;
  shadingsClipped: number;
  annotationsRemoved: number;
  formValuesRemoved: number;
  /** things removed whole because they could not be cut exactly, as sentences */
  removedWholesale: string[];
}

export interface RedactionReport {
  pages: PageReport[];
  /** places that may still show what was removed, as sentences */
  warnings: string[];
}

export interface RedactionPreview {
  areas: AreaPreview[];
  report: RedactionReport;
}

/** a redacted copy opened for the review's pictures; the working copy is untouched. */
export interface DryRun {
  previewId: number;
  pages: PageSize[];
  report: RedactionReport;
}

export interface RedactionEdit {
  document: DocumentState;
  report: RedactionReport;
}

export type ScrubItem =
  | "documentInfo"
  | "xmpMetadata"
  | "attachments"
  | "javaScript"
  | "hiddenLayers"
  | "comments"
  | "formValues"
  | "thumbnails"
  | "pieceInfo"
  | "privateData"
  | "unusedObjects"
  | "externalLinks"
  | "fingerprints";

export interface ScrubFinding {
  item: ScrubItem;
  count: number;
  /** what was found, for people: "Author: Jane Doe", "notes.docx" */
  details: string[];
}

export interface ScrubReport {
  findings: ScrubFinding[];
}

export interface ScrubEdit {
  document: DocumentState;
  report: ScrubReport;
}

export type PrintPermission = "none" | "lowQuality" | "full";

export interface PermissionOptions {
  printing: PrintPermission;
  copying: boolean;
  editing: boolean;
  commenting: boolean;
  fillingForms: boolean;
  assembling: boolean;
}

export interface LockOptions {
  openPassword: string;
  ownerPassword: string;
  permissions: PermissionOptions;
}

export interface FileLock {
  /** "AES-256", "AES-128" or "RC4" */
  method: string;
  revision: number;
  needsOpenPassword: boolean;
  permissions: PermissionOptions;
}

export type PasswordUsed = "open" | "owner" | "none";

/** a document's protection, as the Protect tab shows it. */
export interface ProtectionStatus {
  /** the working copy is still locked: read only until unlocked */
  locked: boolean;
  lock: FileLock | null;
  unlockedWith: PasswordUsed | null;
  /** the password settings were changed (or kept) for the next save */
  changed: boolean;
  openPasswordOnSave: boolean;
  restrictionsOnSave: PermissionOptions | null;
  ownerPasswordSet: boolean;
  /** save waits for a choice about the open password */
  undecided: boolean;
  /** saving writes the whole file from scratch */
  cleanSave: boolean;
  /** content was removed, so saving deletes the versions kept for undo */
  removedContent: boolean;
}

export function findRedactionText(id: number, query: TextQuery, pages: number[] = []): Promise<FoundText[]> {
  return invoke("find_redaction_text", { id, query, pages });
}

export function redactionPreview(id: number, areas: RedactionArea[]): Promise<RedactionPreview> {
  return invoke("redaction_preview", { id, areas });
}

export function redactionDryRun(id: number, areas: RedactionArea[], options: RedactOptions): Promise<DryRun> {
  return invoke("redaction_dry_run", { id, areas, options });
}

export function closeRedactionDryRun(id: number, previewId: number): Promise<void> {
  return invoke("close_redaction_dry_run", { id, previewId });
}

export function applyRedactions(id: number, areas: RedactionArea[], options: RedactOptions): Promise<RedactionEdit> {
  return invoke("apply_redactions", { id, areas, options });
}

export function hiddenInformation(id: number): Promise<ScrubReport> {
  return invoke("hidden_information", { id });
}

export function removeHiddenInformation(id: number, items: ScrubItem[]): Promise<ScrubEdit> {
  return invoke("remove_hidden_information", { id, items });
}

export function protectionStatus(id: number): Promise<ProtectionStatus> {
  return invoke("protection_status", { id });
}

export function unlockDocument(id: number, password: string): Promise<DocumentState> {
  return invoke("unlock_document", { id, password });
}

/** a change to the password settings; what is left out is kept. */
export interface LockChange {
  /** the password to open the file; "" removes it */
  openPassword?: string;
  permissions?: PermissionOptions;
  /** the password that lifts restrictions; "" means one nobody knows */
  ownerPassword?: string;
  /** no password and no restrictions at all */
  removeAll?: boolean;
}

/** changes what save does about a password. the document then counts as changed. */
export function changeLockOnSave(id: number, change: LockChange): Promise<DocumentState> {
  return invoke("change_lock_on_save", { id, change });
}
