// find and replace in the editor document. matches stay inside one
// paragraph (as in word); text a tracked change has deleted is skipped,
// because it is on its way out and replacing it would bring it back.

import type { Node as PmNode } from "prosemirror-model";
import type { Transaction } from "prosemirror-state";

export interface Match {
  from: number;
  to: number;
}

export interface FindOptions {
  matchCase?: boolean;
  wholeWords?: boolean;
}

/** a stand-in for characters that must never match (atoms, deleted text). */
const BLOCKED = "\u0000";

function isDeleted(node: PmNode): boolean {
  return node.marks.some((mark) => mark.type.name === "change" && (mark.attrs.data as { kind?: string }).kind === "deletion");
}

/** a paragraph's searchable text, and the document position of each character. */
function searchableText(block: PmNode, start: number): { text: string; positions: number[] } {
  let text = "";
  const positions: number[] = [];
  block.forEach((child, offset) => {
    const pos = start + offset;
    if (child.isText && !isDeleted(child)) {
      const value = child.text ?? "";
      for (let i = 0; i < value.length; i += 1) positions.push(pos + i);
      text += value;
    } else {
      // an atom is one position; it separates words but never matches
      positions.push(pos);
      text += BLOCKED.repeat(child.isText ? (child.text ?? "").length : 1);
      for (let i = 1; child.isText && i < (child.text ?? "").length; i += 1) positions.push(pos + i);
    }
  });
  return { text, positions };
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** every match of `query`, in document order. */
export function findAll(doc: PmNode, query: string, options: FindOptions = {}): Match[] {
  if (!query) return [];
  const flags = options.matchCase ? "gu" : "giu";
  const body = escapeRegExp(query);
  const pattern = new RegExp(options.wholeWords ? `(?<![\\p{L}\\p{N}_])${body}(?![\\p{L}\\p{N}_])` : body, flags);
  const matches: Match[] = [];
  doc.descendants((node, pos) => {
    if (!node.isTextblock) return true;
    const { text, positions } = searchableText(node, pos + 1);
    for (const found of text.matchAll(pattern)) {
      const index = found.index ?? 0;
      const last = index + found[0].length - 1;
      matches.push({ from: positions[index], to: positions[last] + 1 });
    }
    return false;
  });
  return matches;
}

/** the first match at or after `pos`, wrapping round to the start. */
export function matchFrom(matches: Match[], pos: number): number {
  if (matches.length === 0) return -1;
  const index = matches.findIndex((match) => match.from >= pos);
  return index === -1 ? 0 : index;
}

/** replaces one match, keeping the formatting of its first character. */
export function replaceMatch(tr: Transaction, match: Match, replacement: string): void {
  const marks = tr.doc.resolve(match.from + 1).marks();
  if (replacement) tr.replaceWith(match.from, match.to, tr.doc.type.schema.text(replacement, marks));
  else tr.delete(match.from, match.to);
}

/** replaces every match in one step, so one undo takes them all back. */
export function replaceAll(tr: Transaction, query: string, replacement: string, options: FindOptions = {}): number {
  const matches = findAll(tr.doc, query, options);
  // from the end, so earlier positions stay valid
  for (const match of [...matches].reverse()) replaceMatch(tr, match, replacement);
  return matches.length;
}

/** "3 of 12", "No matches", for the find capsule. */
export function matchLabel(current: number, count: number): string {
  if (count === 0) return "No matches";
  return current >= 0 ? `${current + 1} of ${count}` : `${count} found`;
}
