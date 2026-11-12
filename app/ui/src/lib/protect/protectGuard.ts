// how the window asks before closing a document with marks not applied.
//
// marks for redaction live only in the window, so closing the document
// (going back to Recents, closing the window) would throw them away. the
// window (App.svelte) asks here first; while a document is shown, its
// Protect session puts its question here.

export interface ProtectGuard {
  /** resolves to true when leaving may go ahead (no marks, or the person chose to discard them) */
  mayLeave: (() => Promise<boolean>) | null;
}

export const protectGuard: ProtectGuard = { mayLeave: null };

/** whether the window may close the document, asking the session when it has a question. */
export async function protectMayLeave(guard: ProtectGuard = protectGuard): Promise<boolean> {
  return guard.mayLeave ? guard.mayLeave() : true;
}

/** the words of the question, in the manner of a mac's sheet. */
export function discardMarksQuestion(count: number): { title: string; message: string } {
  return {
    title: count === 1 ? "Discard the area marked for redaction?" : `Discard the ${count} areas marked for redaction?`,
    message: "They haven’t been applied, so nothing has been removed from the document. If you close it now, the marks are lost.",
  };
}
