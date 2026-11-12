// turning what was typed in the on-page editor into the smallest change
// the engine can make.
//
// the smaller the change, the more of the page stays exactly as it was:
// inserting or deleting a few letters keeps the rest of the line's own
// glyphs and spacing, and only the letters after the change move along.
// rewriting a whole line re-sets just that line; only when several lines
// changed is the paragraph re-set and wrapped to its width again.

import type { TextBlock } from "./editApi";

export type TextChange =
  | { kind: "none" }
  | { kind: "insert"; lineId: string; index: number; text: string }
  | { kind: "delete"; lineId: string; start: number; end: number }
  | { kind: "replaceLine"; lineId: string; text: string }
  | { kind: "replaceBlock"; blockId: string; text: string };

/** the text the editor starts with: the block's lines, one per line. */
export function editorText(block: Pick<TextBlock, "lines">): string {
  return block.lines.map((line) => line.text).join("\n");
}

/** how many characters two strings share at the start, and then at the end without overlapping. */
export function sharedEnds(before: string, after: string): { prefix: number; suffix: number } {
  let prefix = 0;
  const shortest = Math.min(before.length, after.length);
  while (prefix < shortest && before[prefix] === after[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < shortest - prefix &&
    before[before.length - 1 - suffix] === after[after.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  return { prefix, suffix };
}

/** the change within one line, as an insert, a delete, or the line rewritten. */
export function lineChange(lineId: string, before: string, after: string): TextChange {
  if (before === after) return { kind: "none" };
  const { prefix, suffix } = sharedEnds(before, after);
  const removed = before.slice(prefix, before.length - suffix);
  const added = after.slice(prefix, after.length - suffix);
  if (removed === "") return { kind: "insert", lineId, index: prefix, text: added };
  if (added === "") return { kind: "delete", lineId, start: prefix, end: prefix + removed.length };
  return { kind: "replaceLine", lineId, text: after };
}

/** the change that turns a block's text into what was typed. */
export function planTextChange(block: Pick<TextBlock, "id" | "lines">, typed: string): TextChange {
  const lines = block.lines;
  const typedLines = typed.split("\n");
  if (typed === editorText(block)) return { kind: "none" };
  if (typedLines.length === lines.length) {
    const changed = lines.map((_, index) => index).filter((index) => lines[index].text !== typedLines[index]);
    if (changed.length === 1) {
      const index = changed[0];
      return lineChange(lines[index].id, lines[index].text, typedLines[index]);
    }
  }
  // lines on a page are wrapped, not separate paragraphs: the engine wraps
  // the words to the block's width again
  const words = typedLines.map((line) => line.trim()).filter((line) => line !== "").join(" ");
  return { kind: "replaceBlock", blockId: block.id, text: words };
}

/** true when nothing would be left of the block: the person cleared it. */
export function clearsBlock(change: TextChange): boolean {
  return change.kind === "replaceBlock" && change.text.trim() === "";
}

/** characters typed that the font can't draw, each once, in the order typed. */
export function missingCharacters(typed: string, original: string, drawable: string): string {
  const known = new Set([...drawable, ...original, " ", "\n"]);
  const missing: string[] = [];
  for (const character of typed) {
    if (!known.has(character) && !missing.includes(character)) missing.push(character);
  }
  return missing.join("");
}
