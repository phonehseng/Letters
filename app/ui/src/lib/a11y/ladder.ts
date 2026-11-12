// the two lists the person can rearrange in the wizard: the heading
// ladder (each heading's level) and the reading order of a page.

import type { HeadingChange, ObjectRef } from "./a11yApi";

export interface LadderRow {
  element: ObjectRef;
  text: string;
  /** the level the heading has now */
  from: number;
  /** the level it will get */
  to: number;
}

export function ladderFrom(proposal: HeadingChange[]): LadderRow[] {
  return proposal.map((change) => ({ element: change.element, text: change.text, from: change.from, to: change.to }));
}

export function setLevel(rows: LadderRow[], index: number, level: number): LadderRow[] {
  const clamped = Math.min(6, Math.max(1, Math.round(level)));
  return rows.map((row, at) => (at === index ? { ...row, to: clamped } : row));
}

/**
 * the headings whose level jumps more than one below the one before (an
 * H1 followed by an H3), as row numbers. the first heading should be an H1.
 */
export function skippedLevels(rows: LadderRow[]): number[] {
  const skipped: number[] = [];
  let previous = 0;
  rows.forEach((row, index) => {
    if (row.to > previous + 1) skipped.push(index);
    previous = row.to;
  });
  return skipped;
}

/** only the rows that change, which is what the fix sends. */
export function ladderChanges(rows: LadderRow[]): { element: ObjectRef; level: number }[] {
  return rows.filter((row) => row.to !== row.from).map((row) => ({ element: row.element, level: row.to }));
}

/** moves one item of a list to another place, as a drag or the arrow keys do. */
export function moveItem<T>(items: T[], from: number, to: number): T[] {
  if (from < 0 || from >= items.length) return items;
  const target = Math.min(items.length - 1, Math.max(0, to));
  if (target === from) return items;
  const next = items.slice();
  const [moved] = next.splice(from, 1);
  next.splice(target, 0, moved);
  return next;
}

export function sameRef(a: ObjectRef, b: ObjectRef): boolean {
  return a.number === b.number && a.generation === b.generation;
}

/** what screen readers hear after a move: "Moved to 2 of 5." */
export function movedText(position: number, count: number): string {
  return `Moved to ${position + 1} of ${count}.`;
}
