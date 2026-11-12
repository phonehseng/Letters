// documents made by New from Images that haven't been saved anywhere yet.
//
// such a document has no file of its own, so Save must ask where to put
// it, as Save As does, and the save panel should start beside the
// pictures it was made from rather than in the app's cache.

const unsaved = new Map<number, string>();

/** remembers a new document and where its save panel should start. */
export function rememberNewDocument(id: number, suggestedPath: string) {
  unsaved.set(id, suggestedPath);
}

/** the place to suggest when a new document is saved, or null for a document with a file. */
export function newDocumentPath(id: number): string | null {
  return unsaved.get(id) ?? null;
}

/** once saved somewhere, it is an ordinary document. */
export function forgetNewDocument(id: number) {
  unsaved.delete(id);
}

/** "C:\\Photos\\IMG_0012.pdf" for the first picture "C:\\Photos\\IMG_0012.jpg". */
export function suggestedPathFor(firstPicture: string): string {
  return firstPicture.replace(/\.[^.\\/]+$/, "") + ".pdf";
}
