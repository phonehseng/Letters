// Convert File…: the types the shell offers for a file, and the words and
// names the sheet shows, without any dom (ConvertSheet.svelte draws them).

export type ConvertFormat = "docx" | "odt" | "xlsx" | "ods" | "csv" | "pptx" | "odp" | "pdf";

/** what kind of file was chosen; pdfs and pictures have tools of their own. */
export type ConvertSource = "document" | "spreadsheet" | "presentation" | "pdf" | "picture" | "other";

export interface ConvertTarget {
  format: ConvertFormat;
  label: string;
  extension: string;
  available: boolean;
  /** how it is made, or why it can't be made on this computer */
  note: string;
}

export interface ConvertChoices {
  source: ConvertSource;
  targets: ConvertTarget[];
}

export interface ConvertedFile {
  path: string;
  /** what the new type couldn't keep, in sentences */
  notes: string[];
}

/** what the file picker shows: the files convert_file.rs reads, plus pdfs and pictures. */
export const CONVERTIBLE_EXTENSIONS = ["docx", "odt", "rtf", "doc", "xlsx", "xlsm", "ods", "csv", "tsv", "pptx", "odp", "pdf", "png", "jpg", "jpeg", "tif", "tiff", "heic"];

/** "C:\School\Essay.docx" as odt is "C:\School\Essay.odt": beside the original, same name. */
export function suggestedPath(path: string, extension: string): string {
  const slash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const dot = path.lastIndexOf(".");
  const base = dot > slash ? path.slice(0, dot) : path;
  return `${base}.${extension}`;
}

/** the file's name without its folder. */
export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** the first type that can be made here, chosen when the list appears. */
export function firstAvailable(targets: ConvertTarget[]): ConvertFormat | null {
  return targets.find((target) => target.available)?.format ?? null;
}

/** what the sheet says about a file it doesn't convert itself, or null. */
export function elsewhereSentence(source: ConvertSource): string | null {
  switch (source) {
    case "pdf":
      return "PDFs have their own Export To…, with a choice of pages: Word, Excel, PowerPoint, pictures, a web page or text.";
    case "picture":
      return "Pictures become a PDF with New from Images…, which can also make their words searchable.";
    case "other":
      return "Letters can’t convert this kind of file. It converts Word, OpenDocument, RTF, Excel, CSV, PowerPoint and PDF files, and pictures.";
    default:
      return null;
  }
}

/** the result line: where the new file is. */
export function savedSentence(converted: ConvertedFile): string {
  const path = converted.path;
  const slash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const folder = slash > 0 ? fileName(path.slice(0, slash)) : "";
  return folder ? `Saved “${fileName(path)}” in ${folder}.` : `Saved “${fileName(path)}”.`;
}
