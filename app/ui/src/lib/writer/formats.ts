// the formats Save As offers, each with one sentence on what it keeps and
// loses. Word and OpenDocument become the document's own file; the others
// are a copy, and the document stays what it was. RTF and Markdown wait
// for the office engine's exporters and are shown as coming.

export type SaveFormatId = "docx" | "odt" | "pdf" | "html" | "txt" | "rtf" | "md";

export interface SaveFormat {
  id: SaveFormatId;
  label: string;
  extension: string;
  /** "own": the document now belongs to the new file; "copy": the document keeps its file */
  becomes: "own" | "copy";
  available: boolean;
  note: string;
}

export const SAVE_FORMATS: SaveFormat[] = [
  { id: "docx", label: "Word Document", extension: "docx", becomes: "own", available: true, note: "Keeps everything, including what Letters can’t edit, so it opens in Word as it was." },
  { id: "odt", label: "OpenDocument Text", extension: "odt", becomes: "own", available: true, note: "Keeps text, styles, lists, tables, notes and pictures; some Word-only items keep only their text." },
  { id: "pdf", label: "PDF", extension: "pdf", becomes: "copy", available: true, note: "Looks the same everywhere and can’t be edited as a document. Word or LibreOffice makes it when installed." },
  { id: "html", label: "Web Page", extension: "html", becomes: "copy", available: true, note: "Keeps text, headings, lists, tables and links; page layout, headers and footers are lost." },
  { id: "txt", label: "Plain Text", extension: "txt", becomes: "copy", available: true, note: "Keeps only the words: all formatting, pictures and tables’ layout are lost." },
  { id: "rtf", label: "Rich Text", extension: "rtf", becomes: "copy", available: false, note: "Coming with the office engine’s RTF export." },
  { id: "md", label: "Markdown", extension: "md", becomes: "copy", available: false, note: "Coming with the office engine’s Markdown export." },
];

/** the format a chosen file name stands for, or null for one Letters can't write. */
export function formatForPath(path: string): SaveFormat | null {
  const extension = (path.split(/[\\/]/).pop() ?? "").split(".").pop()?.toLowerCase() ?? "";
  const format = SAVE_FORMATS.find((each) => each.extension === extension || (each.id === "html" && extension === "htm"));
  return format?.available ? format : null;
}

/** the save panel's list: the document's own format first, then the others it can write. */
export function saveFilters(own: "docx" | "odt"): { name: string; extensions: string[] }[] {
  const available = SAVE_FORMATS.filter((format) => format.available);
  const first = available.filter((format) => format.id === own);
  return [...first, ...available.filter((format) => format.id !== own)].map((format) => ({ name: format.label, extensions: [format.extension] }));
}
