// the Export dialog's logic, without any dom: which formats this computer
// can make (from the shell's capability list), the options the person
// picked, checking them, and turning them into the job the shell runs.

import { parsePageRanges } from "../pageRanges";

/** one conversion, as core/convert's `capabilities()` describes it. */
export interface Capability {
  from: string;
  to: string;
  style: "exactMatch" | "easyToEdit" | null;
  how: string;
  note: string;
  available: boolean;
}

export interface ExportCapabilities {
  capabilities: Capability[];
  exactMatchExplained: string;
  easyToEditExplained: string;
  /** word is installed, so exact match is measured through it */
  wordInstalled?: boolean;
}

export type FormatId = "word" | "openDocument" | "excel" | "powerPoint" | "images" | "webPage" | "text";
export type Style = "exactMatch" | "easyToEdit";

/** one of the two ways a format can be made, with its sentence. */
export interface StyleChoice {
  style: Style;
  label: string;
  available: boolean;
  /** the engine's sentence for the style, or why it can't be used */
  explanation: string;
  /** the engine's note about this conversion */
  note: string;
}

/** one row of the format list. */
export interface FormatChoice {
  id: FormatId;
  label: string;
  available: boolean;
  /** what to expect, or why it can't be made here */
  note: string;
  /** set for formats that come two ways */
  styles?: StyleChoice[];
}

const FORMATS: { id: FormatId; label: string; to: string[] }[] = [
  { id: "word", label: "Word Document", to: ["docx"] },
  { id: "openDocument", label: "OpenDocument Text", to: ["odt"] },
  { id: "excel", label: "Excel Workbook", to: ["xlsx"] },
  { id: "powerPoint", label: "PowerPoint Slides", to: ["pptx"] },
  { id: "images", label: "Images", to: ["png", "jpeg", "tiff"] },
  { id: "webPage", label: "Web Page", to: ["html"] },
  { id: "text", label: "Plain Text", to: ["text"] },
];

/** the words for each style in each format. plain text calls them by what they do. */
function styleLabel(format: FormatId, style: Style): string {
  if (format === "text") return style === "easyToEdit" ? "Reading Order" : "Keep Layout";
  return style === "exactMatch" ? "Exact Match" : "Easy to Edit";
}

/** why a choice can't be used: the engine's own sentence, or "Coming soon." for what isn't built. */
export function unavailableReason(capability: Capability | undefined): string {
  if (!capability) return "Coming soon.";
  if (capability.how === "Not in this version") return `Coming soon. ${capability.note}`;
  return capability.note;
}

/** the format list for the dialog, in a fixed order, from what this computer can do. */
export function formatChoices(found: ExportCapabilities): FormatChoice[] {
  const fromPdf = found.capabilities.filter((capability) => capability.from === "pdf");
  return FORMATS.map((format) => {
    const matching = fromPdf.filter((capability) => format.to.includes(capability.to));
    const styled = matching.filter((capability) => capability.style !== null);
    const styles = styled.length > 1 ? styleChoices(format.id, styled, found) : undefined;
    const available = matching.some((capability) => capability.available);
    const first = matching.find((capability) => capability.available) ?? matching[0];
    return {
      id: format.id,
      label: format.label,
      available,
      note: available ? (first?.note ?? "") : unavailableReason(first),
      styles,
    };
  });
}

function styleChoices(format: FormatId, styled: Capability[], found: ExportCapabilities): StyleChoice[] {
  // exact match first, as the brief asks it to be the default where it can be made
  return (["exactMatch", "easyToEdit"] as Style[]).flatMap((style) => {
    const capability = styled.find((each) => each.style === style);
    if (!capability) return [];
    const sentence = style === "exactMatch" ? found.exactMatchExplained : found.easyToEditExplained;
    return [
      {
        style,
        label: styleLabel(format, style),
        available: capability.available,
        explanation: capability.available ? sentence : unavailableReason(capability),
        note: capability.note,
      },
    ];
  });
}

/** everything the person can set in the dialog. */
export interface ExportOptions {
  format: FormatId;
  /** the chosen style of each format that has two */
  styles: Record<"word" | "webPage" | "text", Style>;
  image: "png" | "jpeg" | "tiff";
  dpi: number;
  color: "color" | "grayscale";
  /** jpeg quality, 1 to 100 */
  quality: number;
  /** a tiff with every page in one file */
  multiPage: boolean;
  /** a web page with its pictures and fonts inside the one file */
  singleFile: boolean;
  excelLayout: "sheetPerPage" | "sheetPerTable";
  pages: "all" | "range";
  /** what was typed for a range, such as "1-3, 7" */
  pagesText: string;
}

/** the starting options. `wordStyle` comes from Settings (Files). */
export function defaultOptions(wordStyle: Style = "exactMatch"): ExportOptions {
  return {
    format: "word",
    styles: { word: wordStyle, webPage: "exactMatch", text: "easyToEdit" },
    image: "png",
    dpi: 150,
    color: "color",
    quality: 85,
    multiPage: true,
    singleFile: true,
    excelLayout: "sheetPerPage",
    pages: "all",
    pagesText: "",
  };
}

/** the style that will be used: the chosen one if it can be made here, else the other. */
export function effectiveStyle(choice: FormatChoice | undefined, chosen: Style): Style {
  const styles = choice?.styles;
  if (!styles) return chosen;
  const picked = styles.find((each) => each.style === chosen);
  if (picked?.available) return chosen;
  return styles.find((each) => each.available)?.style ?? chosen;
}

/** the shell's request for a format (see shell-core/src/export/request.rs). */
export type ExportRequest =
  | { format: "word"; style: Style }
  | { format: "openDocument" }
  | { format: "excel"; layout: "sheetPerPage" | "sheetPerTable" }
  | { format: "powerPoint" }
  | { format: "images"; image: ImageSettings; dpi: number; color: "color" | "grayscale" }
  | { format: "webPage"; style: Style; singleFile: boolean }
  | { format: "text"; style: Style };

export type ImageSettings = { kind: "png" } | { kind: "jpeg"; quality: number } | { kind: "tiff"; compression: "lzw"; multiPage: boolean };

export function requestFor(options: ExportOptions, choices: FormatChoice[] = []): ExportRequest {
  const style = (format: "word" | "webPage" | "text") =>
    effectiveStyle(
      choices.find((choice) => choice.id === format),
      options.styles[format],
    );
  switch (options.format) {
    case "word":
      return { format: "word", style: style("word") };
    case "openDocument":
      return { format: "openDocument" };
    case "excel":
      return { format: "excel", layout: options.excelLayout };
    case "powerPoint":
      return { format: "powerPoint" };
    case "images":
      return { format: "images", image: imageSettings(options), dpi: options.dpi, color: options.color };
    case "webPage":
      return { format: "webPage", style: style("webPage"), singleFile: options.singleFile };
    case "text":
      return { format: "text", style: style("text") };
  }
}

function imageSettings(options: ExportOptions): ImageSettings {
  if (options.image === "jpeg") return { kind: "jpeg", quality: Math.round(options.quality) };
  if (options.image === "tiff") return { kind: "tiff", compression: "lzw", multiPage: options.multiPage };
  return { kind: "png" };
}

/** pages as the shell reads them: every page, or 0-based numbers. */
export type PageSelection = "all" | { pages: number[] };

export type Checked = { ok: true; request: ExportRequest; pages: PageSelection } | { ok: false; error: string };

/** checks the options before the save panel opens, with a sentence that says what to fix. */
export function checkOptions(options: ExportOptions, pageCount: number, choices: FormatChoice[]): Checked {
  const choice = choices.find((each) => each.id === options.format);
  if (choice && !choice.available) return { ok: false, error: choice.note };
  let pages: PageSelection = "all";
  if (options.pages === "range") {
    const parsed = parsePageRanges(options.pagesText, pageCount);
    if (!parsed.ok) return { ok: false, error: parsed.error };
    pages = { pages: parsed.pages };
  }
  const request = requestFor(options, choices);
  if (request.format === "word" && request.style === "exactMatch" && pages !== "all") {
    return { ok: false, error: "Exact Match makes the whole document. Choose All Pages, or Easy to Edit for some pages." };
  }
  if (options.format === "images") {
    if (!(options.dpi >= 10 && options.dpi <= 2400)) return { ok: false, error: "Choose a resolution between 10 and 2400 dpi." };
    if (options.image === "jpeg" && !(options.quality >= 1 && options.quality <= 100)) return { ok: false, error: "Choose a quality between 1 and 100." };
  }
  return { ok: true, request, pages };
}

/** the save panel's filter for the chosen format. */
export function saveFilter(options: ExportOptions): { name: string; extensions: string[] } {
  switch (options.format) {
    case "word":
      return { name: "Word document", extensions: ["docx"] };
    case "openDocument":
      return { name: "OpenDocument text", extensions: ["odt"] };
    case "excel":
      return { name: "Excel workbook", extensions: ["xlsx"] };
    case "powerPoint":
      return { name: "PowerPoint presentation", extensions: ["pptx"] };
    case "webPage":
      return { name: "Web page", extensions: ["html"] };
    case "text":
      return { name: "Plain text", extensions: ["txt"] };
    case "images":
      if (options.image === "jpeg") return { name: "JPEG picture", extensions: ["jpg", "jpeg"] };
      if (options.image === "tiff") return { name: "TIFF picture", extensions: ["tiff", "tif"] };
      return { name: "PNG picture", extensions: ["png"] };
  }
}

/** "Essay.pdf" becomes "Essay.docx" in the same folder. */
export function suggestedName(pdfPath: string, options: ExportOptions): string {
  return pdfPath.replace(/\.[^.\\/]+$/, "") + "." + saveFilter(options).extensions[0];
}

/** how long it may take, said before it starts, for the slow ones. */
export function durationHint(request: ExportRequest, wordInstalled: boolean): string | null {
  if (request.format === "word" && request.style === "exactMatch") {
    return wordInstalled
      ? "This takes from about ten seconds to a minute and a half, because Word itself measures and checks the result."
      : "Word isn’t on this computer, so the result can’t be checked here.";
  }
  return null;
}

/** "Exporting… 40%" or "Exporting page 3…". */
export function progressText(fraction: number, page: number | null): string {
  if (page !== null && page > 0) return `Exporting page ${page}…`;
  return `Exporting… ${Math.round(Math.min(1, Math.max(0, fraction)) * 100)}%`;
}

/** what the shell sends back when an export is done. */
export interface ExportOutcome {
  files: string[];
  summary: string;
  notes: string[];
  verification: string | null;
  closeness: Closeness | null;
  standIns: string[];
}

export interface Closeness {
  glyphs: number;
  maxPt: number;
  meanPt: number;
  p99Pt: number;
  within0025: number;
  within006: number;
  within0125: number;
  pass: boolean;
}

/** the rows of "How close is it?", in plain words. */
export function closenessRows(closeness: Closeness): { label: string; value: string }[] {
  const pt = (value: number) => `${value.toFixed(3)} pt`;
  const share = (value: number) => `${(value * 100).toFixed(1)}%`;
  return [
    { label: "Letters measured", value: closeness.glyphs.toLocaleString("en-US") },
    { label: "Farthest from its place", value: pt(closeness.maxPt) },
    { label: "Average distance", value: pt(closeness.meanPt) },
    { label: "99 in 100 letters within", value: pt(closeness.p99Pt) },
    { label: "Within 0.025 pt", value: share(closeness.within0025) },
    { label: "Within 0.06 pt", value: share(closeness.within006) },
    { label: "Within 0.125 pt", value: share(closeness.within0125) },
    { label: "Meets the exact match standard", value: closeness.pass ? "Yes" : "Not quite" },
  ];
}

/** the one result line: the engine's summary, and its check when it made one. */
export function resultLine(outcome: ExportOutcome): string {
  return outcome.verification ? `${outcome.summary} ${outcome.verification}` : outcome.summary;
}
