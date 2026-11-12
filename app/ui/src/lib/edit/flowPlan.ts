// whether what was typed in a paragraph is a small change inside a line,
// or one that re-wraps the paragraph and moves what is below it.
//
// a small change (a typo, a date) keeps every other glyph of the line as it
// was, so it stays the smallest edit textDiff.ts can make. once the edited
// line would no longer fit its paragraph's width, or several lines changed,
// the paragraph is re-wrapped by the engine (`reflow_paragraph` in core/edit)
// and the column below moves down by whole lines instead of being covered.

import type { TextBlock, TextLine } from "./editApi";
import { planTextChange, type TextChange } from "./textDiff";

export type FlowPlan = TextChange | { kind: "reflow"; blockId: string; text: string };

/**
 * a line may run this far (points) past its paragraph's right edge before
 * it counts as not fitting: glyph boxes and the width estimate differ by
 * about a letter's side bearing.
 */
const FIT_SLACK = 1.5;

/** the paragraph's words on one line each joined by a space: the engine wraps them. */
export function flowingText(typed: string): string {
  return typed
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
    .join(" ");
}

/** how wide a line would be with `length` characters, at its own average width per character. */
export function estimatedWidth(line: TextLine, length: number): number {
  const perCharacter = line.rect.width / Math.max(line.text.length, 1);
  return perCharacter * length;
}

/** the room a line has: from where it starts to the paragraph's right edge. */
function roomFor(block: TextBlock, line: TextLine): number {
  return block.rect.x + block.rect.width - line.rect.x;
}

function lineOf(block: TextBlock, lineId: string): TextLine | null {
  return block.lines.find((line) => line.id === lineId) ?? null;
}

/** the new length of the line a small change touches. */
function newLength(line: TextLine, change: TextChange): number {
  switch (change.kind) {
    case "insert":
      return line.text.length + change.text.length;
    case "replaceLine":
      return change.text.length;
    default:
      return line.text.length;
  }
}

/** the change to make for what was typed: a small one, or a reflow of the paragraph. */
export function planFlow(block: TextBlock, typed: string): FlowPlan {
  const change = planTextChange(block, typed);
  if (change.kind === "none" || change.kind === "delete") return change;
  if (change.kind === "replaceBlock") {
    // clearing the paragraph is a delete, not a reflow
    if (change.text.trim() === "") return change;
    return { kind: "reflow", blockId: block.id, text: change.text };
  }
  const line = lineOf(block, change.lineId);
  if (!line) return change;
  const fits = estimatedWidth(line, newLength(line, change)) <= roomFor(block, line) + FIT_SLACK;
  return fits ? change : { kind: "reflow", blockId: block.id, text: flowingText(typed) };
}
