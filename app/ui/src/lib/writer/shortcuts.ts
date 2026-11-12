// the writer's keyboard shortcuts: the standard word-processor keys.
//
// they are matched before the app's own keys (the writer listens first),
// because a few letters mean something else in the pdf viewer: Ctrl L and
// Ctrl R turn pages there and align text here; Ctrl Alt 1 to 3 show
// sidebar views there and make headings here. the screens never show at
// the same time, so nothing is lost. the keys every screen shares (open,
// save, undo, find, zoom) are not in this table and keep working.

import type { KeyPress } from "../commands";
import type { WriterCommand } from "./writerCommands";

interface Shortcut {
  command: WriterCommand;
  /** the physical key, read from `code` so it works on any layout (Digit1, KeyB) */
  code: string;
  shift?: boolean;
  alt?: boolean;
  /** how the key is written for people, without the ctrl or command part */
  label: string;
}

export const WRITER_SHORTCUTS: Shortcut[] = [
  { command: "format-bold", code: "KeyB", label: "B" },
  { command: "format-italic", code: "KeyI", label: "I" },
  { command: "format-underline", code: "KeyU", label: "U" },
  { command: "format-strike", code: "KeyX", shift: true, label: "Shift X" },
  { command: "style-heading-1", code: "Digit1", alt: true, label: "Alt 1" },
  { command: "style-heading-2", code: "Digit2", alt: true, label: "Alt 2" },
  { command: "style-heading-3", code: "Digit3", alt: true, label: "Alt 3" },
  { command: "style-body", code: "Digit0", alt: true, label: "Alt 0" },
  { command: "align-left", code: "KeyL", label: "L" },
  { command: "align-center", code: "KeyE", label: "E" },
  { command: "align-right", code: "KeyR", label: "R" },
  { command: "align-justify", code: "KeyJ", label: "J" },
  { command: "list-numbers", code: "Digit7", shift: true, label: "Shift 7" },
  { command: "list-bullets", code: "Digit8", shift: true, label: "Shift 8" },
  { command: "indent-more", code: "BracketRight", label: "]" },
  { command: "indent-less", code: "BracketLeft", label: "[" },
  { command: "insert-link", code: "KeyK", label: "K" },
  { command: "insert-page-break", code: "Enter", label: "Enter" },
  { command: "insert-footnote", code: "KeyF", alt: true, label: "Alt F" },
  { command: "insert-comment", code: "KeyM", alt: true, label: "Alt M" },
  { command: "find-replace", code: "KeyH", label: "H" },
  { command: "track-changes", code: "KeyE", shift: true, label: "Shift E" },
  { command: "print", code: "KeyP", label: "P" },
  // a selected picture: in line with the text, or free on the page
  { command: "image-inline", code: "KeyL", alt: true, shift: true, label: "Alt Shift L" },
  { command: "image-free", code: "KeyF", alt: true, shift: true, label: "Alt Shift F" },
];

/** the writer command a key press stands for, or null. ctrl on windows, command on a mac. */
export function writerCommandForKey(press: KeyPress): WriterCommand | null {
  const mod = press.ctrlKey || press.metaKey;
  if (!mod || (press.ctrlKey && press.metaKey)) return null;
  // on many layouts AltGr (which windows reports for ctrl + alt) types a
  // character such as ³ or @: in a text field that character wins. a plain
  // letter or digit means no character was typed, so the shortcut stands
  const typedSymbol = press.key.length === 1 && !/[a-z0-9]/i.test(press.key);
  if (press.altGraph && press.typing && typedSymbol) return null;
  const found = WRITER_SHORTCUTS.find(
    (shortcut) =>
      shortcut.code === press.code && !!shortcut.shift === !!press.shiftKey && !!shortcut.alt === !!press.altKey,
  );
  return found?.command ?? null;
}

/** "Ctrl B" on windows, "⌘B" style on a mac is left to the menu bar. */
export function shortcutLabel(command: WriterCommand): string | undefined {
  const found = WRITER_SHORTCUTS.find((shortcut) => shortcut.command === command);
  return found ? `Ctrl ${found.label}` : undefined;
}
