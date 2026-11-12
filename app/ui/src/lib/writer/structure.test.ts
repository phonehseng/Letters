// outline, find and replace, word counts and the page estimate.
import { describe, expect, it } from "vitest";
import { docToModel } from "./convert";
import { countDocument, countRange, wordCount, wordLabel } from "./counts";
import { findAll, matchFrom, matchLabel, replaceAll, replaceMatch } from "./find";
import { buildOutline, currentEntry, moveSection, sectionBlocks, shiftedLevel } from "./outline";
import { pageCount, pageOfBlock, paginate } from "./pagination";
import { heading, para, select, startOf, stateWith } from "./testDocs";

const essay = () =>
  stateWith([
    heading(1, "Introduction"),
    para("Opening words."),
    heading(2, "Background"),
    para("Some history."),
    heading(1, "Method"),
    para("How it was done."),
    heading(1, "Results"),
  ]);

describe("outline", () => {
  it("lists headings live with their levels", () => {
    const outline = buildOutline(essay().doc);
    expect(outline.map((entry) => [entry.level, entry.text, entry.index])).toEqual([
      [1, "Introduction", 0],
      [2, "Background", 2],
      [1, "Method", 4],
      [1, "Results", 6],
    ]);
    expect(currentEntry(outline, outline[1].pos + 3)?.text).toBe("Background");
    expect(currentEntry(outline, 0)?.text).toBe("Introduction");
  });

  it("a section runs to the next heading of the same or a higher level", () => {
    const doc = essay().doc;
    expect(sectionBlocks(doc, 0)).toEqual({ first: 0, end: 4 });
    expect(sectionBlocks(doc, 2)).toEqual({ first: 2, end: 4 });
    expect(sectionBlocks(doc, 6)).toEqual({ first: 6, end: 7 });
  });

  it("dragging a section moves the heading with everything under it", () => {
    const state = essay();
    const tr = state.tr;
    expect(moveSection(tr, 4, 0)).toBe(true);
    const texts = buildOutline(tr.doc).map((entry) => entry.text);
    expect(texts).toEqual(["Method", "Introduction", "Background", "Results"]);
    expect(tr.doc.child(1).textContent).toBe("How it was done.");
    // moving it down to the end
    const later = state.tr;
    expect(moveSection(later, 0, 7)).toBe(true);
    expect(buildOutline(later.doc).map((entry) => entry.text)).toEqual(["Method", "Results", "Introduction", "Background"]);
    // a drop inside its own section does nothing
    expect(moveSection(state.tr, 0, 2)).toBe(false);
  });

  it("promote and demote stop at the ends", () => {
    expect(shiftedLevel(2, "promote")).toBe(1);
    expect(shiftedLevel(1, "promote")).toBeNull();
    expect(shiftedLevel(6, "demote")).toBeNull();
  });
});

describe("find and replace", () => {
  const doc = () => stateWith([para("The cat sat. The Cat ran."), para("concatenate the cat")]);

  it("finds every match, ignoring case unless asked", () => {
    expect(findAll(doc().doc, "cat")).toHaveLength(4);
    expect(findAll(doc().doc, "cat", { matchCase: true })).toHaveLength(3);
    expect(findAll(doc().doc, "cat", { wholeWords: true })).toHaveLength(3);
    expect(findAll(doc().doc, "")).toEqual([]);
    expect(findAll(doc().doc, "a.")).toHaveLength(0);
  });

  it("matches point at the right text", () => {
    const state = doc();
    const [first] = findAll(state.doc, "sat");
    expect(state.doc.textBetween(first.from, first.to)).toBe("sat");
    expect(matchFrom(findAll(state.doc, "cat"), first.to)).toBe(1);
    expect(matchFrom(findAll(state.doc, "cat"), 10_000)).toBe(0);
  });

  it("replace all is one step and keeps formatting", () => {
    const state = stateWith([{ type: "paragraph", content: [{ type: "text", text: "red red", marks: [{ type: "bold" }] }] }]);
    const tr = state.tr;
    expect(replaceAll(tr, "red", "blue")).toBe(2);
    expect(tr.doc.textContent).toBe("blue blue");
    expect(docToModel(tr.doc).blocks[0].content).toEqual([{ type: "text", text: "blue blue", marks: [{ type: "bold" }] }]);
  });

  it("replaces one match, and skips tracked deleted text", () => {
    const state = stateWith([
      { type: "paragraph", content: [{ type: "text", text: "old " }, { type: "text", text: "old", marks: [{ type: "deletion", id: "1", author: "A" }] }] },
    ]);
    const matches = findAll(state.doc, "old");
    expect(matches).toHaveLength(1);
    const tr = state.tr;
    replaceMatch(tr, matches[0], "new");
    expect(tr.doc.textContent).toBe("new old");
    expect(matchLabel(0, 3)).toBe("1 of 3");
    expect(matchLabel(-1, 0)).toBe("No matches");
  });
});

describe("counts", () => {
  it("counts words as the engine does", () => {
    expect(wordCount("Hello, world — and 42 more")).toBe(5);
    expect(wordCount("  ")).toBe(0);
  });

  it("leaves out deleted text and counts a selection", () => {
    const state = stateWith([
      para("One two three."),
      { type: "paragraph", content: [{ type: "text", text: "four " }, { type: "text", text: "gone", marks: [{ type: "deletion", id: "1", author: "A" }] }] },
    ]);
    expect(countDocument(state.doc).words).toBe(4);
    const selected = select(state, startOf(state, 0), startOf(state, 0) + 7);
    expect(countRange(selected.doc, selected.selection.from, selected.selection.to).words).toBe(2);
    expect(wordLabel(1204, null, "en-US")).toBe("1,204 words");
    expect(wordLabel(1, null)).toBe("1 word");
    expect(wordLabel(1204, 12, "en-US")).toBe("12 of 1,204 words");
  });
});

describe("page estimate", () => {
  it("starts a page where the next block would not fit", () => {
    const pages = paginate([{ height: 400 }, { height: 400 }, { height: 400 }], 900);
    expect(pages.starts).toEqual([0, 2]);
    expect(pageCount(pages)).toBe(2);
    expect(pageOfBlock(pages, 2)).toBe(2);
    expect(pages.lastPageUsed).toBe(400);
  });

  it("page breaks and break-before start pages; a tall block is not split", () => {
    expect(paginate([{ height: 10 }, { height: 0, isPageBreak: true }, { height: 10 }], 900).starts).toEqual([0, 2]);
    expect(paginate([{ height: 10 }, { height: 10, breakBefore: true }], 900).starts).toEqual([0, 1]);
    expect(paginate([{ height: 2000 }, { height: 10 }], 900).starts).toEqual([0, 1]);
    expect(paginate([{ height: 10 }, { height: 0, isPageBreak: true }], 900).starts).toEqual([0, 2]);
    expect(paginate([], 900).starts).toEqual([0]);
  });
});
