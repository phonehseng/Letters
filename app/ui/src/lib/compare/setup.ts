// choosing what to compare: the older and the newer version, before the
// comparison starts. from a document, the open document is the newer one
// and the person picks the older; from Recents they pick (or drop) both.

import { fileNameOf } from "../pageApi";
import type { CompareInput } from "./compareApi";

/** one chosen version, with the name people know it by. */
export interface ChosenVersion {
  input: CompareInput;
  name: string;
}

export interface CompareSetup {
  older: ChosenVersion | null;
  newer: ChosenVersion | null;
}

/** what the window asks the Compare screen to start with. */
export function setupFromDocument(document: { id: number; name: string }): CompareSetup {
  return { older: null, newer: { input: { kind: "open", id: document.id }, name: document.name } };
}

export const EMPTY_SETUP: CompareSetup = { older: null, newer: null };

export function chosenFile(path: string): ChosenVersion {
  return { input: { kind: "file", path }, name: fileNameOf(path) };
}

export function swap(setup: CompareSetup): CompareSetup {
  return { older: setup.newer, newer: setup.older };
}

export function isReady(setup: CompareSetup): setup is { older: ChosenVersion; newer: ChosenVersion } {
  return setup.older !== null && setup.newer !== null;
}

/**
 * pdfs dropped on the screen: two fill both versions (the first dropped is
 * the older); one fills whichever is empty, the older first.
 */
export function withDropped(setup: CompareSetup, paths: string[]): CompareSetup {
  const pdfs = paths.filter((path) => /\.pdf$/i.test(path));
  if (pdfs.length >= 2) return { older: chosenFile(pdfs[0]), newer: chosenFile(pdfs[1]) };
  if (pdfs.length === 1) {
    if (!setup.older) return { ...setup, older: chosenFile(pdfs[0]) };
    return { ...setup, newer: chosenFile(pdfs[0]) };
  }
  return setup;
}

/** the line under the Compare button: what will happen, or what is missing. */
export function setupEffect(setup: CompareSetup): string {
  if (!setup.older && !setup.newer) return "Choose the older and the newer version, or drop two PDFs here.";
  if (!setup.older) return "Choose the older version to compare with.";
  if (!setup.newer) return "Choose the newer version to compare with.";
  return `Finds what changed from “${setup.older.name}” to “${setup.newer.name}”. Neither file changes.`;
}
