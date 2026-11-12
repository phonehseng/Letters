// how the window asks the writer before it closes or goes back to Recents.
//
// the window (App.svelte) owns the unsaved changes sheet; the writer owns
// whether there are unsaved changes and how to save them. while a writer
// document is shown it puts its answer here, and the window calls it.

import type { SaveChoice } from "../documentState";

export interface WriterGuard {
  /** whether leaving may go ahead, after asking with `ask` when there are unsaved changes */
  mayLeave: ((ask: (name: string) => Promise<SaveChoice>) => Promise<boolean>) | null;
}

export const writerGuard: WriterGuard = { mayLeave: null };

/** the same rules as mayLeave in documentState.ts, for a document saved by the writer. */
export async function writerMayLeave(
  document: { name: string; dirty: boolean },
  ask: (name: string) => Promise<SaveChoice>,
  save: () => Promise<boolean>,
  dontSave: () => Promise<void>,
): Promise<boolean> {
  if (!document.dirty) return true;
  const choice = await ask(document.name);
  if (choice === "cancel") return false;
  if (choice === "dont-save") {
    await dontSave();
    return true;
  }
  return save();
}
