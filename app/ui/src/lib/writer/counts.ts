// the status bar's numbers: words in the document (or in the selection),
// counted the way core/office/src/counts.rs counts them. text a tracked
// change has deleted does not count.

import type { Node as PmNode } from "prosemirror-model";

/** a word is a run of non-space characters with a letter or digit in it, so a lone dash is not a word. */
export function wordCount(text: string): number {
  return text.split(/\s+/u).filter((word) => /[\p{L}\p{N}]/u.test(word)).length;
}

function visibleText(node: PmNode): string {
  if (!node.isText) return node.isLeaf ? " " : "";
  const deleted = node.marks.some((mark) => mark.type.name === "change" && (mark.attrs.data as { kind?: string }).kind === "deletion");
  return deleted ? " " : (node.text ?? "");
}

/** the visible text between two positions, paragraphs separated by line breaks. */
export function textBetween(doc: PmNode, from: number, to: number): string {
  let text = "";
  doc.nodesBetween(from, to, (node, pos) => {
    if (node.isTextblock && text) text += "\n";
    if (!node.isInline) return true;
    if (node.isText) {
      const start = Math.max(from, pos) - pos;
      const end = Math.min(to, pos + node.nodeSize) - pos;
      const visible = visibleText(node);
      text += visible === " " ? " " : visible.slice(start, end);
    } else {
      text += visibleText(node);
    }
    return false;
  });
  return text;
}

export interface Counts {
  words: number;
  characters: number;
}

export function countRange(doc: PmNode, from: number, to: number): Counts {
  const text = textBetween(doc, from, to);
  return { words: wordCount(text), characters: Array.from(text.replace(/\n/g, "")).length };
}

export function countDocument(doc: PmNode): Counts {
  return countRange(doc, 0, doc.content.size);
}

/** "1,204 words", "1 word", "12 of 1,204 words" for a selection. */
export function wordLabel(total: number, selected: number | null, locale?: string): string {
  const format = (value: number) => value.toLocaleString(locale);
  const unit = (value: number) => (value === 1 ? "word" : "words");
  if (selected !== null) return `${format(selected)} of ${format(total)} ${unit(total)}`;
  return `${format(total)} ${unit(total)}`;
}
