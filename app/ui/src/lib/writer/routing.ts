// which screen a file opens in. the shell decides the same way
// (app/shell-core/src/writer/routing.rs); the ui asks first so it can open
// the right screen without a wasted round trip.

export type FileRoute = "viewer" | "writer" | "unsupported";

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
  return `Letters can’t open “${name}” yet. It opens PDFs and Word (.docx) or OpenDocument (.odt) documents.`;
}

/** what kind of document a recent file is, in words people know. */
export function documentKind(path: string): string {
  const extension = extensionOf(path);
  if (extension === "docx") return "Word document";
  if (extension === "odt") return "OpenDocument text";
  if (extension === "rtf") return "Rich Text document";
  if (extension === "doc") return "Word 97–2003 document";
  return "PDF";
}

/** the line under a recent file: "12 pages", or "Word document" for documents without a page count. */
export function recentDetails(path: string, pageCount: number, pages: (count: number) => string): string {
  return routeForPath(path) === "writer" ? documentKind(path) : pages(pageCount);
}
