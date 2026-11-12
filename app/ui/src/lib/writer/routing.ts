// which screen a file opens in. the shell decides the same way
// (app/shell-core/src/writer/routing.rs); the ui asks first so it can open
// the right screen without a wasted round trip.

// spreadsheets: "sheets" opens the spreadsheet screen (lib/sheets)
export type FileRoute = "viewer" | "writer" | "sheets" | "unsupported";

export function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function routeForPath(path: string): FileRoute {
  const extension = extensionOf(path);
  if (extension === "pdf") return "viewer";
  // rich text and old word files open by conversion (the shell makes a .docx beside them)
  if (["docx", "odt", "rtf", "doc"].includes(extension)) return "writer";
  // spreadsheets: workbooks and delimited text open in the sheets screen (lib/sheets)
  if (["xlsx", "xlsm", "ods", "csv", "tsv"].includes(extension)) return "sheets";
  return "unsupported";
}

export function nameOf(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** a plain sentence for a file Letters can't open yet. */
export function unsupportedMessage(path: string): string {
  const name = nameOf(path);
  if (extensionOf(path) === "doc") {
    return `“${name}” is an older Word document. Open it in Word and save it as .docx, then open that here.`;
  }
  // every screen letters has: the viewer, the writer, sheets, slides, new from images and the code editor
  return `Letters can’t open “${name}”. It opens PDFs, Word and OpenDocument documents, Excel and PowerPoint files, pictures, and text and code files.`;
}

/** what kind of document a recent file is, in words people know. */
export function documentKind(path: string): string {
  const extension = extensionOf(path);
  if (extension === "docx") return "Word document";
  if (extension === "odt") return "OpenDocument text";
  if (extension === "rtf") return "Rich Text document";
  if (extension === "doc") return "Word 97–2003 document";
  // presentations (lib/slides)
  if (extension === "pptx") return "PowerPoint presentation";
  if (extension === "odp") return "OpenDocument presentation";
  // spreadsheets (lib/sheets)
  if (extension === "xlsx" || extension === "xlsm") return "Excel workbook";
  if (extension === "ods") return "OpenDocument spreadsheet";
  if (extension === "csv" || extension === "tsv") return "CSV file";
  return "PDF";
}

/** the line under a recent file: "12 pages", or "Word document" for documents without a page count. */
export function recentDetails(path: string, pageCount: number, pages: (count: number) => string): string {
  // presentations: Recents keeps their slide count (lib/slides)
  if (["pptx", "odp"].includes(extensionOf(path))) return pageCount === 1 ? "1 slide" : `${pageCount} slides`;
  // spreadsheets show their kind too (lib/sheets)
  return routeForPath(path) === "writer" || routeForPath(path) === "sheets" ? documentKind(path) : pages(pageCount);
}
