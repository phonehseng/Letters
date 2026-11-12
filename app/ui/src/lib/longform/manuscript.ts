// the manuscript navigator's numbers: how many words each chapter and
// section holds, from the writer's own outline (headings) and the words
// between them. the outline itself stays the writer's (writer/outline.ts);
// this only adds counts beside its entries.

export interface Heading {
  /** where the heading is in the document's blocks */
  blockIndex: number;
  level: number;
  text: string;
}

export interface SectionCount {
  blockIndex: number;
  level: number;
  text: string;
  /** words from this heading to the next heading of the same or a higher level */
  words: number;
  /** words under this heading before any sub-heading */
  ownWords: number;
}

/** counts words the way the status bar does: runs with a letter or digit. */
export function countWords(text: string): number {
  return text.split(/\s+/).filter((word) => /[\p{L}\p{N}]/u.test(word)).length;
}

/**
 * each heading's section size. `blockWords[i]` is the words in block i.
 * a chapter's count includes its sections, as a novelist reads it.
 */
export function sectionCounts(headings: Heading[], blockWords: number[]): SectionCount[] {
  const prefix = [0];
  for (const words of blockWords) prefix.push(prefix[prefix.length - 1] + words);
  const total = (from: number, to: number) => prefix[Math.min(to, blockWords.length)] - prefix[Math.min(from, blockWords.length)];
  return headings.map((heading, i) => {
    const nextAny = headings[i + 1]?.blockIndex ?? blockWords.length;
    const end = headings.slice(i + 1).find((h) => h.level <= heading.level)?.blockIndex ?? blockWords.length;
    // the heading's own words are not part of its section's prose
    const own = total(heading.blockIndex + 1, nextAny);
    return { ...heading, words: total(heading.blockIndex + 1, end) - headingWordsBetween(headings, i, end, blockWords), ownWords: own };
  });
}

/** the words of sub-headings inside a section, which are titles and not prose. */
function headingWordsBetween(headings: Heading[], index: number, end: number, blockWords: number[]): number {
  return headings
    .slice(index + 1)
    .filter((h) => h.blockIndex < end)
    .reduce((sum, h) => sum + (blockWords[h.blockIndex] ?? 0), 0);
}

/** "12,400 words" or "1 word". */
export function wordsLabel(words: number): string {
  return words === 1 ? "1 word" : `${words.toLocaleString("en-US")} words`;
}

/**
 * the new order of blocks when a section (a heading and everything until
 * the next heading of its level or higher) moves before another heading.
 * returns the list of block indexes in their new order, or null when the
 * move makes no sense (a section dropped inside itself).
 */
export function moveSection(headings: Heading[], blockCount: number, moving: number, before: number | null): number[] | null {
  const from = headings[moving];
  if (!from) return null;
  const end = headings.slice(moving + 1).find((h) => h.level <= from.level)?.blockIndex ?? blockCount;
  const target = before === null ? blockCount : headings[before]?.blockIndex;
  if (target === undefined || (target > from.blockIndex && target < end)) return null;
  const order = Array.from({ length: blockCount }, (_, i) => i);
  const section = order.slice(from.blockIndex, end);
  const rest = order.filter((i) => i < from.blockIndex || i >= end);
  const at = rest.indexOf(target);
  if (at === -1) return [...rest, ...section];
  return [...rest.slice(0, at), ...section, ...rest.slice(at)];
}
