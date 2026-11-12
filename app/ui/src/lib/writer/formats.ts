// the formats Save As offers, from the office engine's table
// (formats::format_capabilities, through writerApi.saveFormats), each with
// the engine's sentence on what it keeps. Word and OpenDocument become the
// document's own file; the others are a copy and the document stays what
// it was. PDF is not a document format of the engine: Word or LibreOffice
// make it (bridge::print_pdf), so its entry is added here.

import type { FormatCapability } from "./writerApi";

export type SaveFormatId = "docx" | "odt" | "pdf" | "html" | "txt" | "rtf" | "md" | "doc";

export interface SaveFormat {
  id: SaveFormatId;
  label: string;
  extension: string;
  /** "own": the document now belongs to the new file; "copy": the document keeps its file */
  becomes: "own" | "copy";
  available: boolean;
  note: string;
}

const ID_FOR_ENGINE: Record<FormatCapability["format"], SaveFormatId> = {
  docx: "docx",
  odt: "odt",
  rtf: "rtf",
  html: "html",
  plainText: "txt",
  markdown: "md",
  doc: "doc",
};

const PDF_NOTE = "Looks the same everywhere and can’t be edited as a document. Word makes it with Word’s exact pages; without Word, LibreOffice makes a close copy.";

/** the engine's table as Save As shows it; `converter` is true when Word or LibreOffice is installed. */
export function saveFormatsFrom(capabilities: FormatCapability[], converter: boolean): SaveFormat[] {
  const formats: SaveFormat[] = capabilities
    .filter((capability) => capability.canSave)
    .map((capability) => {
      const id = ID_FOR_ENGINE[capability.format];
      const available = !capability.needsConverter || converter;
      return {
        id,
        label: capability.label,
        extension: capability.extension,
        becomes: id === "docx" || id === "odt" ? "own" : "copy",
        available,
        note: available ? capability.summary : `${capability.summary} Word or LibreOffice isn’t on this computer, so this isn’t available.`,
      };
    });
  // pdf goes after the two document formats, where people look for it
  const at = Math.min(2, formats.length);
  formats.splice(at, 0, { id: "pdf", label: "PDF", extension: "pdf", becomes: "copy", available: true, note: PDF_NOTE });
  return formats;
}

/** the format a chosen file name stands for, or null for one Letters can't write here. */
export function formatForPath(path: string, formats: SaveFormat[]): SaveFormat | null {
  const extension = (path.split(/[\\/]/).pop() ?? "").split(".").pop()?.toLowerCase() ?? "";
  const format = formats.find((each) => each.extension === extension || (each.id === "html" && extension === "htm") || (each.id === "md" && extension === "markdown"));
  return format?.available ? format : null;
}

/** the save panel's list: the document's own format first, then the others it can write. */
export function saveFilters(own: "docx" | "odt", formats: SaveFormat[]): { name: string; extensions: string[] }[] {
  const available = formats.filter((format) => format.available);
  const first = available.filter((format) => format.id === own);
  return [...first, ...available.filter((format) => format.id !== own)].map((format) => ({ name: format.label, extensions: [format.extension] }));
}

/** the sentence after saving a copy: what was written and that the document stays itself. */
export function copySavedSentence(format: SaveFormat, documentName: string): string {
  const pictures = format.id === "md" ? " Its pictures are in the “images” folder beside it." : "";
  return `Saved a copy as ${format.label}.${pictures} This document is still “${documentName}”.`;
}
