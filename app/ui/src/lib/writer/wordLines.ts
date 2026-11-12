// Strict Word mode: lines break where word breaks them.
//
// word's layout (writerApi.wordLayout) gives, for every line of the main
// text, its paragraph and the character offset where it starts. each
// top-level paragraph gets a forced break at those places and no other
// wrapping (objectLayer.ts draws them), so the words on each line are
// word's. a paragraph is left to the browser when the match is in doubt:
// word's line text must start where the offset points, or the paragraph
// keeps its own wrapping (a field's hidden code or a picture word counts
// differently would otherwise shift every later line).

import type { Node as PmNode } from "prosemirror-model";
import type { WordLines } from "./objectLayer";
import { editorOffset } from "./textBlocks";
import type { WordLine } from "./writerApi";

/** the first word of a line of text, for comparing word's lines with the editor's. */
export function firstWord(text: string): string {
  const match = text.replace(/[\u0000-\u001f￼]/g, " ").match(/[\p{L}\p{N}][\p{L}\p{N}'’\-]*/u);
  return match ? match[0] : "";
}

/** the editor's text from `pos` to the end of its paragraph, atoms as object characters. */
function textFrom(paragraph: PmNode, offset: number): string {
  return paragraph.textBetween(offset, paragraph.content.size, undefined, "￼");
}

/** whether the paragraph text at `offset` starts where word's line does. */
export function lineMatches(paragraph: PmNode, offset: number, wordText: string): boolean {
  const expected = firstWord(wordText);
  if (!expected) return true;
  const actual = firstWord(textFrom(paragraph, offset));
  return actual === expected || actual.startsWith(expected) || expected.startsWith(actual);
}

/** each top-level paragraph's forced line breaks, from word's lines; paragraphs in doubt are left out. */
export function wordLineBreaks(doc: PmNode, lines: WordLine[]): WordLines[] {
  const byBlock = new Map<number, WordLine[]>();
  for (const line of lines) {
    if (line.cell) continue;
    const list = byBlock.get(line.block) ?? [];
    list.push(line);
    byBlock.set(line.block, list);
  }
  const result: WordLines[] = [];
  let pos = 0;
  doc.forEach((node, offset, index) => {
    pos = offset;
    const found = byBlock.get(index);
    if (!node.isTextblock || !found || found.length < 2) return;
    const breaks: number[] = [];
    for (const line of found.slice(1)) {
      const at = editorOffset(node, line.offset);
      if (at <= 0 || at >= node.content.size || !lineMatches(node, at, line.text)) return;
      breaks.push(pos + 1 + at);
    }
    result.push({ pos, breaks: [...new Set(breaks)].sort((a, b) => a - b) });
  });
  return result;
}

/** how many of word's lines start with the same word on screen: the proof figure. */
export function matchingShare(wordFirstWords: string[], screenFirstWords: string[]): number {
  if (wordFirstWords.length === 0) return 1;
  let same = 0;
  wordFirstWords.forEach((word, index) => {
    if (word === screenFirstWords[index]) same += 1;
  });
  return same / wordFirstWords.length;
}
