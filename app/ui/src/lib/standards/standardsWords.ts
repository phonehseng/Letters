// the plain words of Prepare & Check: what each pdf/a level is for, how a
// finding can be fixed, sizes a person can read, and the one-line results.
// the engine's own sentences (each finding's message, the summaries and
// CHECKER_NOTE) are shown as they are; these only label them.

import type { ConversionReport, Finding, Fix, OptimizeReport, PdfaLevel, PdfaReport, PdfxLevel } from "./standardsApi";

export const PDFA_LABELS: Record<PdfaLevel, string> = {
  a1b: "PDF/A-1b",
  a2b: "PDF/A-2b",
  a2u: "PDF/A-2u",
  a3b: "PDF/A-3b",
};

export const PDFA_HINTS: Record<PdfaLevel, string> = {
  a1b: "The oldest kind, for archives that ask for it by name. No transparency or layers.",
  a2b: "What most archives and universities ask for: the look is kept for good.",
  a2u: "As 2b, and every word can be copied and searched as text.",
  a3b: "As 2b, and other files may be attached, such as the spreadsheet behind a chart.",
};

export const PDFX_LABELS: Record<PdfxLevel, string> = {
  x1a: "PDF/X-1a",
  x4: "PDF/X-4",
};

export const PDFX_HINTS: Record<PdfxLevel, string> = {
  x1a: "Older print shops: CMYK colour only, no transparency, every font embedded.",
  x4: "Most print shops today: colour managed, transparency allowed.",
};

export const FIX_LABELS: Record<Fix, string> = {
  automatic: "Letters fixes this when converting",
  needsFont: "Needs the font on this computer",
  needsRendering: "Letters turns this area into a picture",
  notPossible: "Can’t be fixed here; change the original",
};

/** "1.2 MB", "340 KB", "820 bytes". */
export function readableSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} bytes`;
}

/** where a finding is: "Page 3, 12 0 R", "Page 3" or "The whole file". */
export function findingPlace(finding: Finding): string {
  const page = finding.page === null ? null : `Page ${finding.page + 1}`;
  if (page && finding.object) return `${page}, ${finding.object}`;
  return page ?? (finding.object ? `Object ${finding.object}` : "The whole file");
}

/** one line about a check. */
export function checkHeadline(report: PdfaReport): string {
  const level = PDFA_LABELS[report.level];
  if (report.compliant) return `Letters’ checker found nothing against ${level}.`;
  const count = report.findings.length;
  const fixable = report.automaticallyFixable;
  const fixes = fixable === count ? "Converting can fix all of them." : `Converting can fix ${fixable} by itself.`;
  return `${count} ${count === 1 ? "problem" : "problems"} against ${level}. ${fixes}`;
}

/** one line about a conversion; the engine's summary follows it. */
export function conversionHeadline(report: ConversionReport): string {
  const level = PDFA_LABELS[report.level];
  return report.compliant ? `Saved as ${level}.` : `Saved, but not yet ${level}: ${report.blockers.length} ${report.blockers.length === 1 ? "problem remains" : "problems remain"}.`;
}

/** "Saved a copy of 1.2 MB, 38% smaller." */
export function optimizeHeadline(report: OptimizeReport): string {
  const saved = report.bytesBefore - report.bytesAfter;
  if (saved <= 0) return `The copy is ${readableSize(report.bytesAfter)}; Letters found nothing to make smaller.`;
  const percent = Math.round((saved / report.bytesBefore) * 100);
  return `Saved a copy of ${readableSize(report.bytesAfter)}, ${percent}% smaller than ${readableSize(report.bytesBefore)}.`;
}

/** "Essay.pdf" becomes "Essay (PDF-A-2b).pdf" beside it; a slash can't be in a file name. */
export function convertedName(name: string, level: PdfaLevel): string {
  return `${name.replace(/\.pdf$/i, "")} (${PDFA_LABELS[level].replace("/", "-")}).pdf`;
}
