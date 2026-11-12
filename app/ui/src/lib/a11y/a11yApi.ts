// every call the Accessibility tab makes into the shell, typed, and the
// shape of a check's answer as the engine (core/a11y) sends it. pages are
// counted from 0; rectangles are in points on the page as shown, from its
// top left, y down.

import { invoke } from "@tauri-apps/api/core";
import { save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { DocumentState } from "../api";

export type Severity = "mustFix" | "shouldFix" | "checkByHand";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ObjectRef {
  number: number;
  generation: number;
}

export interface ContentRef {
  page: number;
  opIndex: number;
}

export type Target = { kind: "element"; element: ObjectRef } | { kind: "image"; image: ContentRef };

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export type FixKind =
  | { kind: "addTags" }
  | { kind: "markAsTagged" }
  | { kind: "setLanguage"; suggested: string | null }
  | { kind: "setTitle"; suggested: string | null }
  | { kind: "showTitle" }
  | { kind: "setAltText"; target: Target }
  | { kind: "fixHeadingLevels" }
  | { kind: "setTableHeaders"; table: ObjectRef; suggestedHeaderRows: number; suggestedHeaderColumns: number }
  | { kind: "setReadingOrder"; page: number; proposal: ObjectRef[] }
  | { kind: "setFieldTooltip"; field: ObjectRef; suggested: string | null }
  | { kind: "setTabOrder" }
  | { kind: "addLinkText"; annotation: ObjectRef; suggested: string | null }
  | { kind: "setAnnotationContents"; annotation: ObjectRef }
  | { kind: "tagAnnotations" }
  | { kind: "tagUntaggedContent"; page: number }
  | { kind: "changeTextColour"; runs: ContentRef[]; suggested: Rgb }
  | { kind: "addBookmarksFromHeadings" }
  | { kind: "recogniseText"; pages: number[] };

export interface Location {
  page: number | null;
  rect: Rect | null;
  structurePath: string | null;
  element: ObjectRef | null;
  annotation: ObjectRef | null;
}

export interface ContrastMeasurement {
  ratio: number;
  required: number;
  textColour: Rgb;
  backgroundColour: Rgb;
  suggestedColour: Rgb;
}

export interface Finding {
  /** a stable rule id like "missing-alt-text" */
  id: string;
  severity: Severity;
  title: string;
  explanation: string;
  whyItMatters: string;
  fixDescription: string;
  details: string;
  where: Location;
  fix: FixKind | null;
  pdfua: string;
  contrast: ContrastMeasurement | null;
}

export interface Score {
  percent: number;
  passed: number;
  failed: number;
  notApplicable: number;
  notChecked: number;
  byHand: number;
  statement: string;
}

export interface RuleOutcome {
  id: string;
  title: string;
  status: "passed" | "failed" | "notApplicable" | "notChecked" | "byHand";
  reason: string | null;
}

export interface Report {
  pageCount: number;
  findings: Finding[];
  summary: { mustFix: number; shouldFix: number; checkByHand: number; total: number; fixable: number };
  score: Score;
  rules: RuleOutcome[];
  /** the engine's rule for offering Mark as PDF/UA: no must-fix left, nothing it needs unchecked */
  machineChecksPass: boolean;
}

export interface TagDecision {
  page: number;
  rect: Rect | null;
  element: ObjectRef | null;
  decision: string;
  confidence: "medium" | "low";
}

export interface FixOutcome {
  changes: string[];
  decisions: TagDecision[];
  counts: { headings: number; paragraphs: number; lists: number; figures: number; tables: number; artifacts: number } | null;
}

export interface FixEdit {
  document: DocumentState;
  outcome: FixOutcome;
}

export interface HeadingChange {
  element: ObjectRef;
  text: string;
  from: number;
  to: number;
}

export interface ReadingBlock {
  element: ObjectRef;
  role: string;
  rect: Rect | null;
  text: string;
}

/** the engine's own sentences, so the tab and the engine never say different things. */
export interface A11yWords {
  tags: string;
  altText: string;
  readingOrder: string;
  pdfUa: string;
  humanChecks: string[];
}

/** a fix the person confirmed, with their answer (see shell-core's a11y/fixes.rs). */
export type FixRequest =
  | { kind: "addTags"; language: string | null; replaceExisting?: boolean }
  | { kind: "markAsTagged" }
  | { kind: "setLanguage"; language: string }
  | { kind: "setTitle"; title: string }
  | { kind: "showTitle" }
  | { kind: "setAltText"; target: Target; text: string }
  | { kind: "markAsArtifact"; target: Target }
  | { kind: "fixHeadingLevels" }
  | { kind: "setHeadingLevels"; levels: { element: ObjectRef; level: number }[] }
  | { kind: "setTableHeaders"; table: ObjectRef; headerRows: number; headerColumns: number }
  | { kind: "setReadingOrder"; page: number; order: ObjectRef[] }
  | { kind: "setFieldTooltip"; field: ObjectRef; text: string }
  | { kind: "setTabOrder" }
  | { kind: "addLinkText"; annotation: ObjectRef; text: string }
  | { kind: "setAnnotationContents"; annotation: ObjectRef; text: string }
  | { kind: "tagAnnotations" }
  | { kind: "tagUntaggedContent"; page: number }
  | { kind: "changeTextColour"; runs: ContentRef[]; colour: Rgb }
  | { kind: "addBookmarksFromHeadings" }
  | { kind: "markPdfua" };

export function checkDocument(id: number): Promise<Report> {
  return invoke("a11y_check", { id });
}

export function applyFix(id: number, request: FixRequest): Promise<FixEdit> {
  return invoke("a11y_fix", { id, request });
}

export function headingProposal(id: number): Promise<HeadingChange[]> {
  return invoke("a11y_heading_proposal", { id });
}

export function readingBlocks(id: number, page: number, order: ObjectRef[] | null): Promise<ReadingBlock[]> {
  return invoke("a11y_reading_blocks", { id, page, order });
}

export function regionPicture(id: number, page: number, rect: Rect): Promise<string> {
  return invoke("a11y_region_picture", { id, page, rect });
}

export function a11yWords(): Promise<A11yWords> {
  return invoke("a11y_words");
}

export function saveA11yReport(id: number, path: string, format: "html" | "text"): Promise<string> {
  return invoke("a11y_save_report", { id, path, format });
}

export async function chooseA11yReportLocation(suggestedPath: string, format: "html" | "text"): Promise<string | null> {
  const filter = format === "html" ? { name: "Web pages", extensions: ["html"] } : { name: "Plain text", extensions: ["txt"] };
  const chosen = await showSaveDialog({ defaultPath: suggestedPath, filters: [filter] });
  return typeof chosen === "string" ? chosen : null;
}
