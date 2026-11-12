// the commands of Search All Documents, the page finder and the long
// document aids, with their keys. they join the one list in ../commands.ts.
//
// keys were chosen so they shadow nothing already taken:
// - ctrl (command) + shift + g: Search All Documents. ctrl + shift + f
//   was the natural key, but it already opens Find a Topic
// - ctrl (command) + g: Go To, as in word
// - alt (option) + left and right: Back and Forward, as in a browser.
//   command + [ and ] were not used: the writer indents with them
// - ctrl (command) + d: Add Bookmark, as in Preview
// - ctrl (command) + shift + enter: Focus in the writer

import type { KeyPress } from "../commands";

export const READING_COMMANDS = ["search-all-documents", "go-to-page", "history-back", "history-forward", "add-bookmark", "focus-mode"] as const;

export type ReadingCommand = (typeof READING_COMMANDS)[number];

/** the command for a key press with ctrl or command held, or null. */
export function readingCommandForKey(press: KeyPress): ReadingCommand | null {
  if (press.altKey) return null;
  const key = press.key.toLowerCase();
  if (key === "g") return press.shiftKey ? "search-all-documents" : "go-to-page";
  if (key === "d" && !press.shiftKey && !press.typing) return "add-bookmark";
  if (key === "enter" && press.shiftKey) return "focus-mode";
  return null;
}

/** alt (option) + left or right, outside text fields. */
export function readingPlainKey(press: KeyPress): ReadingCommand | null {
  if (press.typing || press.ctrlKey || press.metaKey || !press.altKey || press.shiftKey) return null;
  if (press.key === "ArrowLeft") return "history-back";
  if (press.key === "ArrowRight") return "history-forward";
  return null;
}
