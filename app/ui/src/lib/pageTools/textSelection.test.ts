import { describe, expect, it } from "vitest";
import {
  caretAt,
  caretMoveForKey,
  compareCarets,
  extendByKey,
  extendSelection,
  granularityFor,
  isCollapsed,
  lineSpanAt,
  moveCaret,
  selectedPages,
  selectedText,
  selectionAtPageStart,
  selectionRects,
  startSelection,
  wordSpan,
  type PageText,
  type TextLookup,
} from "./textSelection";

/** a page of lines laid out left to right, each letter 10 points wide and 20 tall, lines 30 apart. */
function pageOf(lines: string[], top = 100): PageText {
  const chars: PageText["chars"] = [];
  const boxes: PageText["lines"] = [];
  lines.forEach((line, index) => {
    const y = top + 30 * index;
    [...line].forEach((ch, column) => chars.push({ ch, rect: { x: 72 + 10 * column, y, width: 10, height: 20 }, line: index }));
    boxes.push({ x: 72, y, width: 10 * line.length, height: 20 });
  });
  return { chars, lines: boxes };
}

/** a page turned a quarter: one line running down the screen. */
function turnedPage(word: string): PageText {
  const chars = [...word].map((ch, index) => ({ ch, rect: { x: 600, y: 72 + 10 * index, width: 20, height: 10 }, line: 0 }));
  return { chars, lines: [{ x: 600, y: 72, width: 20, height: 10 * word.length }] };
}

const notes = pageOf(["Cells need light", "to make sugar."]);
const pages: Record<number, PageText> = {
  0: notes,
  1: pageOf(["Second page"]),
  2: pageOf([]),
  3: pageOf(["Last words"]),
};
const lookup: TextLookup = (page) => pages[page];

/** the selection a drag from one point to another makes. */
function drag(fromPage: number, from: { x: number; y: number }, toPage: number, to: { x: number; y: number }, clicks = 1) {
  const start = startSelection(pages[fromPage], caretAt(pages[fromPage], fromPage, from), granularityFor(clicks));
  return extendSelection(start, pages[toPage], caretAt(pages[toPage], toPage, to));
}

describe("placing the caret", () => {
  it("snaps to the nearer side of the character under the pointer", () => {
    // "C" spans 72 to 82: its left half puts the caret before it
    expect(caretAt(notes, 0, { x: 74, y: 110 })).toEqual({ page: 0, offset: 0 });
    expect(caretAt(notes, 0, { x: 80, y: 110 })).toEqual({ page: 0, offset: 1 });
  });

  it("uses the nearest line when the pointer is between or beside lines", () => {
    // right of the first line: after its last character
    expect(caretAt(notes, 0, { x: 500, y: 110 })).toEqual({ page: 0, offset: 16 });
    // just above the second line
    expect(caretAt(notes, 0, { x: 74, y: 128 })).toEqual({ page: 0, offset: 16 });
    // far left of everything, level with line two
    expect(caretAt(notes, 0, { x: 0, y: 140 }).offset).toBe(16);
  });

  it("works along a line that runs down a turned page", () => {
    const turned = turnedPage("cell");
    expect(caretAt(turned, 0, { x: 610, y: 74 }).offset).toBe(0);
    expect(caretAt(turned, 0, { x: 610, y: 98 }).offset).toBe(3);
    expect(caretAt(turned, 0, { x: 610, y: 400 }).offset).toBe(4);
  });

  it("puts the caret at the start of a page without text", () => {
    expect(caretAt(pages[2], 2, { x: 100, y: 100 })).toEqual({ page: 2, offset: 0 });
  });
});

describe("selecting by dragging", () => {
  it("selects the characters dragged across", () => {
    const selection = drag(0, { x: 72, y: 110 }, 0, { x: 121, y: 110 });
    expect(selectedText(selection, lookup)).toBe("Cells");
    expect(selectionRects(selection, 0, notes)).toEqual([{ x: 72, y: 100, width: 50, height: 20 }]);
  });

  it("works backwards too", () => {
    const selection = drag(0, { x: 121, y: 110 }, 0, { x: 72, y: 110 });
    expect(selectedText(selection, lookup)).toBe("Cells");
    expect(compareCarets(selection.anchor, selection.focus)).toBeGreaterThan(0);
  });

  it("runs across lines with one box per line", () => {
    const selection = drag(0, { x: 132, y: 110 }, 0, { x: 91, y: 140 });
    expect(selectedText(selection, lookup)).toBe("need light\nto");
    expect(selectionRects(selection, 0, notes)).toEqual([
      { x: 132, y: 100, width: 100, height: 20 },
      { x: 72, y: 130, width: 20, height: 20 },
    ]);
  });

  it("runs across pages, taking every page in between whole", () => {
    const selection = drag(0, { x: 153, y: 140 }, 3, { x: 111, y: 110 });
    expect(selectedPages(selection)).toEqual([0, 1, 2, 3]);
    expect(selectedText(selection, lookup)).toBe("sugar.\nSecond page\nLast");
    expect(selectionRects(selection, 1, pages[1])).toEqual([{ x: 72, y: 100, width: 110, height: 20 }]);
    expect(selectionRects(selection, 2, pages[2])).toEqual([]);
  });

  it("is empty after a click without a drag", () => {
    const selection = drag(0, { x: 100, y: 110 }, 0, { x: 101, y: 110 });
    expect(isCollapsed(selection)).toBe(true);
    expect(isCollapsed(null)).toBe(true);
  });
});

describe("words and lines", () => {
  it("finds a word, a run of spaces, or one punctuation mark", () => {
    expect(wordSpan(notes, 7)).toEqual({ start: 6, end: 10 });
    expect(wordSpan(notes, 5)).toEqual({ start: 5, end: 6 });
    // the full stop after "sugar"
    expect(wordSpan(notes, 29)).toEqual({ start: 29, end: 30 });
    // a word never runs across a line
    expect(wordSpan(notes, 15)).toEqual({ start: 11, end: 16 });
  });

  it("keeps words like don't and x-ray whole", () => {
    const page = pageOf(["don't x-ray"]);
    expect(wordSpan(page, 1)).toEqual({ start: 0, end: 5 });
    expect(wordSpan(page, 7)).toEqual({ start: 6, end: 11 });
  });

  it("selects a word on double click and grows by whole words when dragged", () => {
    const word = drag(0, { x: 135, y: 110 }, 0, { x: 135, y: 110 }, 2);
    expect(selectedText(word, lookup)).toBe("need");
    const grown = drag(0, { x: 135, y: 110 }, 0, { x: 185, y: 110 }, 2);
    expect(selectedText(grown, lookup)).toBe("need light");
    const backwards = drag(0, { x: 135, y: 110 }, 0, { x: 80, y: 110 }, 2);
    expect(selectedText(backwards, lookup)).toBe("Cells need");
  });

  it("selects a line on triple click and grows by whole lines", () => {
    const line = drag(0, { x: 135, y: 110 }, 0, { x: 135, y: 110 }, 3);
    expect(selectedText(line, lookup)).toBe("Cells need light");
    const two = drag(0, { x: 135, y: 110 }, 0, { x: 80, y: 140 }, 3);
    expect(selectedText(two, lookup)).toBe("Cells need light\nto make sugar.");
    expect(lineSpanAt(notes, 20)).toEqual({ start: 16, end: 30 });
  });

  it("counts clicks into what they select", () => {
    expect(granularityFor(1)).toBe("character");
    expect(granularityFor(2)).toBe("word");
    expect(granularityFor(3)).toBe("line");
  });
});

describe("the keyboard", () => {
  it("reads the arrow keys, with ctrl or option for whole words", () => {
    expect(caretMoveForKey("ArrowRight", false)).toBe("right");
    expect(caretMoveForKey("ArrowRight", true)).toBe("wordRight");
    expect(caretMoveForKey("ArrowUp", false)).toBe("up");
    expect(caretMoveForKey("End", false)).toBe("lineEnd");
    expect(caretMoveForKey("a", false)).toBeNull();
  });

  it("moves by characters and words", () => {
    expect(moveCaret({ page: 0, offset: 0 }, "right", lookup, 4)).toEqual({ page: 0, offset: 1 });
    expect(moveCaret({ page: 0, offset: 0 }, "wordRight", lookup, 4)).toEqual({ page: 0, offset: 5 });
    expect(moveCaret({ page: 0, offset: 8 }, "wordLeft", lookup, 4)).toEqual({ page: 0, offset: 6 });
    expect(moveCaret({ page: 0, offset: 6 }, "wordLeft", lookup, 4)).toEqual({ page: 0, offset: 0 });
  });

  it("moves up and down a line, keeping the column", () => {
    expect(moveCaret({ page: 0, offset: 2 }, "down", lookup, 4)).toEqual({ page: 0, offset: 18 });
    expect(moveCaret({ page: 0, offset: 18 }, "up", lookup, 4)).toEqual({ page: 0, offset: 2 });
  });

  it("crosses to the next page with text, skipping empty ones", () => {
    expect(moveCaret({ page: 0, offset: 30 }, "right", lookup, 4)).toEqual({ page: 1, offset: 0 });
    expect(moveCaret({ page: 1, offset: 11 }, "right", lookup, 4)).toEqual({ page: 3, offset: 0 });
    expect(moveCaret({ page: 3, offset: 0 }, "left", lookup, 4)).toEqual({ page: 1, offset: 11 });
    expect(moveCaret({ page: 0, offset: 20 }, "down", lookup, 4).page).toBe(1);
  });

  it("stays put at the very start and end", () => {
    expect(moveCaret({ page: 0, offset: 0 }, "left", lookup, 4)).toEqual({ page: 0, offset: 0 });
    expect(moveCaret({ page: 3, offset: 10 }, "right", lookup, 4)).toEqual({ page: 3, offset: 10 });
  });

  it("grows a selection with shift and the arrows, starting from the top of a page", () => {
    let selection = selectionAtPageStart(0);
    selection = extendByKey(selection, "wordRight", lookup, 4);
    expect(selectedText(selection, lookup)).toBe("Cells");
    selection = extendByKey(selection, "down", lookup, 4);
    expect(selectedText(selection, lookup)).toBe("Cells need light\nto ma");
  });

  it("makes a word selection grow by characters from the keyboard", () => {
    const word = drag(0, { x: 135, y: 110 }, 0, { x: 135, y: 110 }, 2);
    const grown = extendByKey(word, "right", lookup, 4);
    expect(grown.anchor).toEqual({ page: 0, offset: 6 });
    expect(grown.focus).toEqual({ page: 0, offset: 11 });
    expect(grown.granularity).toBe("character");
  });
});
