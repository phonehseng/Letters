// the rules around a document that can change: its title, keeping the
// reader's place after an edit, running edits one at a time, and the
// question asked before unsaved changes would be lost.
//
// nothing here touches the screen, so all of it is unit tested.

import type { DocumentState } from "./api";
import type { Anchor } from "./layout";

/**
 * the window's title. like a mac, an edited document says so in the
 * title until it is saved: "essay.pdf — Edited".
 */
export function windowTitle(document: DocumentState | null): string {
  if (!document) return "Recents";
  return document.dirty ? `${document.name} — Edited` : document.name;
}

/**
 * where the reader should be after an edit.
 *
 * the same spot on the same page when that page still exists (a page
 * that turned is the same page, so the reader stays on it). if the
 * document got shorter and the page is gone, the top of the new last page.
 */
export function placeAfterEdit(anchor: Anchor, pageCount: number): Anchor {
  if (pageCount === 0) return { page: 0, fraction: 0 };
  if (anchor.page < pageCount) return anchor;
  return { page: pageCount - 1, fraction: 0 };
}

/** what turning the current page asks the shell for. */
export function rotation(direction: "left" | "right", currentPage: number): { pages: number[]; degrees: number } {
  return { pages: [currentPage], degrees: direction === "left" ? -90 : 90 };
}

/**
 * runs changes to a document one after another, never two at once.
 *
 * if someone presses rotate three times quickly, the three answers must
 * be shown in the order the changes were made, or the screen could end
 * up showing an older state than the file has. each task starts only
 * when the one before it has finished, whether it worked or not.
 */
export function createSerialQueue() {
  let last: Promise<unknown> = Promise.resolve();
  return function run<T>(task: () => Promise<T>): Promise<T> {
    const next = last.then(task, task);
    // the next task waits for this one, but never inherits its failure
    last = next.catch(() => undefined);
    return next;
  };
}

/**
 * the one queue of document changes in this window. edits, undo and redo
 * from the viewer and saving from the window all wait in it, so a save
 * never overtakes the edit before it.
 */
export const documentChanges = createSerialQueue();

/** the three answers to the unsaved changes question. */
export type SaveChoice = "save" | "dont-save" | "cancel";

/** the words of the unsaved changes sheet, as on a mac. */
export function unsavedChangesQuestion(name: string): { title: string; message: string } {
  return {
    title: `Do you want to save the changes made to “${name}”?`,
    message: "Your changes will be lost if you don’t save them.",
  };
}

/** Enter means Save (the prominent button) and Escape means Cancel. */
export function choiceForKey(key: string): SaveChoice | null {
  if (key === "Enter") return "save";
  if (key === "Escape") return "cancel";
  return null;
}

/**
 * whether leaving the document (going back to Recents, closing the
 * window) may go ahead, after asking when there are unsaved changes.
 *
 * `ask` shows the sheet and `save` saves, returning false when saving
 * failed (its own error message is shown elsewhere). a failed save keeps
 * the document open, so nothing is lost.
 */
export async function mayLeave(
  document: DocumentState | null,
  ask: (name: string) => Promise<SaveChoice>,
  save: () => Promise<boolean>,
): Promise<boolean> {
  if (!document?.dirty) return true;
  const choice = await ask(document.name);
  if (choice === "cancel") return false;
  if (choice === "dont-save") return true;
  return save();
}
