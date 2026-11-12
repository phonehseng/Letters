// pages in Free mode, Word's lines and pages applied to the editor, and the clipboard helpers
import { describe, expect, it } from "vitest";
import { paginate, NOTE_SEPARATOR, type BlockBox } from "./pagination";
import { pagesFromWord, sinceLabel, wordPageStarts } from "./layoutSource";
import { firstWord, lineMatches, matchingShare, wordLineBreaks } from "./wordLines";
import { copiedBlocks, mergeNumbering, needsEngineHtml } from "./clipboardBridge";
import { NodeSelection } from "prosemirror-state";
import { para, startOf, stateWith } from "./testDocs";
import type { WordLine } from "./writerApi";
import type { DocumentMeta } from "./model";

/** a paragraph of `count` lines of 20px. */
const lines = (count: number, extra: Partial<BlockBox> = {}): BlockBox => ({ height: count * 20, lineBottoms: Array.from({ length: count }, (_, index) => (index + 1) * 20), ...extra });

describe("pages in Free mode", () => {
  it("a paragraph taller than the room left is split between lines", () => {
    const pages = paginate([lines(3), lines(10)], 200);
    // 60 used, 140 left: 7 lines fit, 3 go on
    expect(pages.breaks).toEqual([
      { block: 0, line: 0 },
      { block: 1, line: 7 },
    ]);
    expect(pages.used).toEqual([200, 60]);
  });

  it("a paragraph taller than a page runs over several pages", () => {
    const pages = paginate([lines(25)], 200);
    expect(pages.breaks?.map((each) => each.line)).toEqual([0, 10, 20]);
    expect(pages.starts).toEqual([0, 0, 0]);
  });

  it("widows and orphans: at least two lines on each side", () => {
    // room for 1 line only: the whole paragraph moves (no orphan)
    expect(paginate([lines(9), lines(5)], 200).breaks).toEqual([{ block: 0, line: 0 }, { block: 1, line: 0 }]);
    // room for 4 of 5: one would be left alone, so 3 stay and 2 go (no widow)
    expect(paginate([lines(6), lines(5)], 200).breaks).toEqual([{ block: 0, line: 0 }, { block: 1, line: 3 }]);
    // three lines can never split
    expect(paginate([lines(9), lines(3)], 200).breaks).toEqual([{ block: 0, line: 0 }, { block: 1, line: 0 }]);
  });

  it("keep lines together moves the whole paragraph", () => {
    expect(paginate([lines(5), lines(6, { keepTogether: true })], 200).breaks?.[1]).toEqual({ block: 1, line: 0 });
  });

  it("a heading goes to the next page with the paragraph after it", () => {
    const pages = paginate([lines(8), { height: 30, keepWithNext: true }, lines(3, { keepTogether: true })], 200);
    expect(pages.breaks?.[1]).toEqual({ block: 1, line: 0 });
    expect(pages.used).toEqual([160, 90]);
  });

  it("page breaks and break-before still work", () => {
    expect(paginate([{ height: 10 }, { height: 0, isPageBreak: true }, { height: 10 }], 200).starts).toEqual([0, 2]);
    expect(paginate([{ height: 10 }, { height: 10, breakBefore: true }], 200).starts).toEqual([0, 1]);
  });

  it("footnotes take room on the page of their reference", () => {
    // 9 lines would fit, but the note on line 2 takes 40px plus the separator
    const pages = paginate([lines(10, { notes: [{ line: 1, height: 40 }] })], 200);
    expect(pages.notes?.[0]).toBe(40 + NOTE_SEPARATOR);
    expect(pages.breaks?.[1].line).toBe(7);
  });
});

describe("pages and lines from Word", () => {
  const wordLines: WordLine[] = [
    { page: 1, block: 0, offset: 0, top: 0, left: 0, text: "First paragraph of" },
    { page: 1, block: 0, offset: 19, top: 0, left: 0, text: "text that wraps" },
    { page: 2, block: 0, offset: 35, top: 0, left: 0, text: "onto page two" },
    { page: 2, block: 1, offset: 0, top: 0, left: 0, text: "Second" },
    { page: 3, block: 2, cell: [0, 0, 0], offset: 0, top: 0, left: 0, text: "cell" },
  ];

  it("a page that starts inside a paragraph starts at Word's character", () => {
    expect(wordPageStarts({ blockPages: [1, 2, 3], pageCount: 3, lines: wordLines })).toEqual([
      { block: 0, line: 0, offset: 0 },
      { block: 0, line: 0, offset: 35 },
    ]);
    expect(pagesFromWord({ blockPages: [1, 1, 2], pageCount: 2 }, 0).starts).toEqual([0, 2]);
  });

  it("each paragraph breaks its lines where Word does, checked against Word's text", () => {
    const text = "First paragraph of text that wraps onto page two";
    const state = stateWith([para(text), para("Second")]);
    const [first] = wordLineBreaks(state.doc, wordLines);
    expect(first.pos).toBe(0);
    expect(first.breaks).toEqual([startOf(state, 0) + 19, startOf(state, 0) + 35]);
    expect(state.doc.textBetween(first.breaks[0], first.breaks[1])).toBe("text that wraps ");
    // a paragraph whose text doesn't match Word's keeps its own wrapping
    const other = stateWith([para("Something else entirely, long enough to wrap twice"), para("Second")]);
    expect(wordLineBreaks(other.doc, wordLines)).toEqual([]);
  });

  it("compares first words, and works out the share that match", () => {
    expect(firstWord("  “Hello, world")).toBe("Hello");
    expect(firstWord("\tdon’t stop")).toBe("don’t");
    expect(lineMatches(stateWith([para("abc def")]).doc.child(0), 4, "def ghi")).toBe(true);
    expect(matchingShare(["a", "b", "c", "d"], ["a", "b", "x", "d"])).toBe(0.75);
  });

  it("says how long ago, plainly", () => {
    expect(sinceLabel(0, 10_000)).toBe("a moment ago");
    expect(sinceLabel(0, 60_000)).toBe("1 minute ago");
    expect(sinceLabel(0, 7_200_000)).toBe("2 hours ago");
  });
});

describe("the clipboard", () => {
  it("a selected picture is copied as a paragraph holding it", () => {
    const state = stateWith([{ type: "paragraph", content: [{ type: "image", mediaId: "a.png", width: 10, height: 10, description: "Dots" }] }]);
    const blocks = copiedBlocks(NodeSelection.create(state.doc, 1));
    expect(blocks).toEqual([{ type: "paragraph", content: [{ type: "image", mediaId: "a.png", width: 10, height: 10, description: "Dots" }] }]);
    expect(copiedBlocks(state.selection)).toBeNull();
  });

  it("html with pictures from other apps goes through the engine; Letters' own copies don't", () => {
    expect(needsEngineHtml('<p>x <img src="file:///C:/a.png"></p>')).toBe(true);
    expect(needsEngineHtml('<p data-json="{}">x <img data-json="{}" src="data:,"></p>')).toBe(false);
    expect(needsEngineHtml("<p>no pictures</p>")).toBe(false);
  });

  it("pasted lists join the document's, without doubles", () => {
    const meta = { numbering: { definitions: [{ id: "1", levels: [] }], instances: [{ id: "1", definitionId: "1" }] } } as unknown as DocumentMeta;
    const merged = mergeNumbering(meta, { definitions: [{ id: "1", levels: [] }, { id: "html-bullets", levels: [] }], instances: [{ id: "h1", definitionId: "html-bullets" }] })!;
    expect(merged.definitions.map((each) => each.id)).toEqual(["1", "html-bullets"]);
    expect(merged.instances.map((each) => each.id)).toEqual(["1", "h1"]);
    expect(mergeNumbering(meta, { definitions: [{ id: "1", levels: [] }] })).toBeNull();
  });
});
