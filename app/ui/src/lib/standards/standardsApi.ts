// the calls of Prepare & Check, typed as core/standards reports them (see
// core/standards/README.md). checks read the document as shown; converting
// and making a smaller copy save a new file and leave the document alone.

import { invoke } from "@tauri-apps/api/core";
import { save as showSaveDialog } from "@tauri-apps/plugin-dialog";

export type PdfaLevel = "a1b" | "a2b" | "a2u" | "a3b";
export type PdfxLevel = "x1a" | "x4";
export type Fix = "automatic" | "needsFont" | "needsRendering" | "notPossible";

export interface Finding {
  rule: string;
  clause: string;
  message: string;
  page: number | null;
  object: string | null;
  fix: Fix;
}

export interface PdfaReport {
  level: PdfaLevel;
  compliant: boolean;
  findings: Finding[];
  automaticallyFixable: number;
  /** CHECKER_NOTE, shown as it is */
  note: string;
}

export interface FontOutcome {
  font: string;
  embeddedAs: string | null;
  widthsDiffer: boolean;
  problem: string | null;
}

export interface ConversionReport {
  level: PdfaLevel;
  compliant: boolean;
  changes: string[];
  fonts: FontOutcome[];
  rasterised: { page: number; wholePage: boolean }[];
  outputIntentAdded: boolean;
  blockers: Finding[];
  before: PdfaReport;
  after: PdfaReport;
  summary: string;
}

export interface WrittenCopy<R> {
  path: string;
  report: R;
  checkerNote: string;
}

export interface PreflightReport {
  structure: Finding[];
  fonts: { name: string; embedded: boolean; kind: string }[];
  images: { page: number; pixelsPerInch: number; width: number; height: number }[];
  pages: { page: number; rotate: number }[];
  size: { part: string; bytes: number }[];
  portability: Finding[];
  summary: string;
}

export interface OptimizeOptions {
  maxPixelsPerInch: number | null;
  objectStreams: boolean;
}

export interface OptimizeReport {
  bytesBefore: number;
  bytesAfter: number;
  measures: { name: string; bytesSaved: number }[];
  notDone: string[];
}

export interface PdfxReport {
  level: PdfxLevel;
  passes: boolean;
  findings: Finding[];
  spotColours: string[];
  note: string;
}

export function checkPdfa(id: number, level: PdfaLevel): Promise<PdfaReport> {
  return invoke("standards_check_pdfa", { id, level });
}

export function convertPdfa(id: number, level: PdfaLevel, destination: string): Promise<WrittenCopy<ConversionReport>> {
  return invoke("standards_convert_pdfa", { id, level, destination });
}

export function preflight(id: number): Promise<PreflightReport> {
  return invoke("standards_preflight", { id });
}

export function optimize(id: number, options: OptimizeOptions, destination: string): Promise<WrittenCopy<OptimizeReport>> {
  return invoke("standards_optimize", { id, options, destination });
}

export function checkPdfx(id: number, level: PdfxLevel): Promise<PdfxReport> {
  return invoke("standards_check_pdfx", { id, level });
}

/** the save panel for a new pdf beside the document. */
export async function choosePdfDestination(suggested: string): Promise<string | null> {
  const chosen = await showSaveDialog({ defaultPath: suggested, filters: [{ name: "PDF documents", extensions: ["pdf"] }] });
  return typeof chosen === "string" ? chosen : null;
}
