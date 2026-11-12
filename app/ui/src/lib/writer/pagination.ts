// where page boundaries go on the editing surface (Free mode, and Strict
// mode whenever word's layout isn't there yet).
//
// this is an estimate, and the status bar says so: word places page
// breaks with its own line breaking and font metrics. Letters measures
// each top-level block on screen, line by line, and follows word's rules
// for where a page may end:
// - a block taller than the room left is split between two lines, so a
//   long paragraph (or one taller than a page) runs on to the next page;
// - widow and orphan control: at least two lines of a paragraph stay
//   together at the bottom of a page and at the top of the next;
// - keep with next: a heading moves to the next page with the paragraph
//   after it; keep lines together moves a whole paragraph;
// - a footnote goes on the page of its reference, and the page's text
//   area shrinks by the notes it holds.

export interface BlockBox {
  /** height in css pixels at 100% zoom, margins between blocks included */
  height: number;
  /** a page break block, or a paragraph set to start on a new page */
  breakBefore?: boolean;
  /** a page break block itself takes no room on either page */
  isPageBreak?: boolean;
  /**
   * where each line of a paragraph ends, from the block's top (css
   * pixels); with two or more, the block can be split between lines
   */
  lineBottoms?: number[];
  /** the paragraph stays on the same page as the next block (headings) */
  keepWithNext?: boolean;
  /** the paragraph's lines are never split between pages */
  keepTogether?: boolean;
  /** footnotes referenced in the block: on which line, and how tall each note is */
  notes?: { line: number; height: number }[];
}

/** where one page starts: a block, and a line inside it when the page starts mid-paragraph. */
export interface PageStart {
  block: number;
  line: number;
}

export interface Pages {
  /** index of the first block that starts on each page; page 1 starts at 0 */
  starts: number[];
  /** where each page starts, lines included (same length as `starts`) */
  breaks?: PageStart[];
  /** how much of each page's text area the text uses, in css pixels */
  used?: number[];
  /** the height of the footnotes on each page */
  notes?: number[];
  /** how much of the last page is used, in css pixels */
  lastPageUsed: number;
}

/** the room a note separator takes on a page with notes (the short rule and its spacing). */
export const NOTE_SEPARATOR = 18;

/** lines that must stay together at a page's bottom and top (word's widow and orphan control). */
const MIN_LINES = 2;

export function paginate(blocks: BlockBox[], pageHeight: number): Pages {
  const breaks: PageStart[] = [{ block: 0, line: 0 }];
  const used: number[] = [0];
  const notes: number[] = [0];
  const page = () => breaks.length - 1;
  const newPage = (block: number, line: number) => {
    breaks.push({ block, line });
    used.push(0);
    notes.push(0);
  };
  const noteCost = (amount: number) => (amount > 0 && notes[page()] === 0 ? amount + NOTE_SEPARATOR : amount);
  const room = () => pageHeight - used[page()] - notes[page()];

  blocks.forEach((block, index) => {
    if (index > 0 && block.breakBefore && used[page()] > 0) newPage(index, 0);
    if (block.isPageBreak) {
      if (index + 1 < blocks.length) newPage(index + 1, 0);
      return;
    }
    const lines = block.lineBottoms ?? [];
    const notesOn = (from: number, to: number) => (block.notes ?? []).filter((note) => note.line >= from && note.line < to).reduce((sum, note) => sum + note.height, 0);
    let firstLine = 0;
    let top = 0;
    // place the rest of the block, splitting it while it is too tall for the room left
    for (;;) {
      const rest = block.height - top;
      const restNotes = noteCost(notesOn(firstLine, Number.MAX_SAFE_INTEGER));
      if (rest + restNotes <= room() + 0.5) {
        used[page()] += rest;
        notes[page()] += restNotes;
        return;
      }
      const split = lines.length - firstLine >= MIN_LINES * 2 && !block.keepTogether ? splitLine(lines, firstLine, top, room(), (from, to) => noteCost(notesOn(from, to))) : null;
      if (split === null) {
        if (used[page()] > 0) {
          newPage(index, firstLine);
          continue;
        }
        // alone on an empty page and still too tall: it runs past the bottom
        used[page()] += rest;
        notes[page()] += restNotes;
        return;
      }
      used[page()] += lines[split - 1] - top;
      notes[page()] += noteCost(notesOn(firstLine, split));
      top = lines[split - 1];
      firstLine = split;
      newPage(index, split);
    }
  });
  // a page break as the last block still makes an (empty) page
  const last = blocks[blocks.length - 1];
  if (last?.isPageBreak) newPage(blocks.length, 0);
  keepWithNext(blocks, breaks, used);
  const unique = dedupe(breaks, used, notes);
  return {
    starts: unique.breaks.map((each) => each.block),
    breaks: unique.breaks,
    used: unique.used,
    notes: unique.notes,
    lastPageUsed: unique.used[unique.used.length - 1] ?? 0,
  };
}

/**
 * the line a split goes before: as many lines as fit in `room`, but at
 * least two on each side; null when no split is allowed here.
 */
function splitLine(lines: number[], firstLine: number, top: number, room: number, notesBetween: (from: number, to: number) => number): number | null {
  let fit = firstLine;
  while (fit < lines.length && lines[fit] - top + notesBetween(firstLine, fit + 1) <= room + 0.5) fit += 1;
  // `fit` lines (counted from firstLine) fit on this page
  const onThisPage = fit - firstLine;
  if (onThisPage < MIN_LINES) return null;
  const onNext = lines.length - fit;
  if (onNext >= MIN_LINES) return fit;
  const pulled = lines.length - MIN_LINES;
  return pulled - firstLine >= MIN_LINES ? pulled : null;
}

/** a page that starts right after a keep-with-next block (a heading) takes that block along. */
function keepWithNext(blocks: BlockBox[], breaks: PageStart[], used: number[]) {
  for (let page = 1; page < breaks.length; page += 1) {
    const start = breaks[page];
    if (start.line !== 0) continue;
    let block = start.block;
    while (block - 1 > breaks[page - 1].block && blocks[block - 1]?.keepWithNext && !blocks[block - 1]?.isPageBreak) block -= 1;
    if (block === start.block) continue;
    const moved = blocks.slice(block, start.block).reduce((sum, each) => sum + each.height, 0);
    used[page - 1] -= moved;
    used[page] += moved;
    breaks[page] = { block, line: 0 };
  }
}

function dedupe(breaks: PageStart[], used: number[], notes: number[]) {
  const keep: number[] = [];
  breaks.forEach((each, index) => {
    const previous = keep.length ? breaks[keep[keep.length - 1]] : null;
    if (previous && previous.block === each.block && previous.line === each.line) {
      used[keep[keep.length - 1]] += used[index];
      return;
    }
    keep.push(index);
  });
  return { breaks: keep.map((index) => breaks[index]), used: keep.map((index) => used[index]), notes: keep.map((index) => notes[index]) };
}

export function pageCount(pages: Pages): number {
  return pages.starts.length;
}

/** "Page 2 of 5" style estimate for the status bar. */
export function pageLabel(current: number, total: number): string {
  return `Page ${current} of about ${total}`;
}

/** the page a block is on, counted from 1. */
export function pageOfBlock(pages: Pages, blockIndex: number): number {
  let page = 1;
  for (let i = 1; i < pages.starts.length; i += 1) if (pages.starts[i] <= blockIndex) page = i + 1;
  return page;
}
