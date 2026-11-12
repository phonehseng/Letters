// tests for the page finder; the same cases as core/search/src/jump.rs.
import { describe, expect, it } from "vitest";
import { resolveJump, type JumpDocument, type JumpEntry } from "./jump";

// the same book and cases as core/search/src/jump.rs, so both stay in step
function book(): JumpDocument {
  const labels = ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi", "xii"];
  for (let n = 1; n <= 288; n++) labels.push(String(n));
  labels[299] = "A-3";
  const entry = (title: string, page: number, level: number): JumpEntry => ({ title, page, level });
  return {
    pageCount: 300,
    currentPage: 40,
    pageLabels: labels,
    outline: [entry("Preface", 2, 0), entry("Chapter 1: Cells", 12, 0), entry("Methods", 20, 1), entry("Chapter 2: Energy", 40, 0), entry("3. Photosynthesis", 80, 0)],
    bookmarks: [entry("Exam formula", 150, 0)],
    lastPosition: 77,
    wordStarts: [],
  };
}

const pagesOf = (doc: JumpDocument, typed: string) => resolveJump(doc, typed).map((c) => [c.kind, c.page]);

describe("the page finder", () => {
  it("reads plain numbers and page prefixes", () => {
    const doc = book();
    for (const typed of ["12", "p 12", "p12", "page 12", "pg. 12", "12/300", "12 of 300", "  P 12 "]) {
      expect(pagesOf(doc, typed), typed).toEqual([
        ["label", 23],
        ["page", 11],
      ]);
    }
    expect(resolveJump(doc, "12")[0].label).toBe("Page 24 (printed 12)");
    expect(resolveJump(doc, "301")).toEqual([]);
  });

  it("finds printed labels like roman numerals", () => {
    const doc = book();
    expect(pagesOf(doc, "page xii")).toEqual([["label", 11]]);
    expect(pagesOf(doc, "iv")).toEqual([["label", 3]]);
    expect(pagesOf(doc, "a-3")).toEqual([["label", 299]]);
  });

  it("reads percentages, relative moves and keywords", () => {
    const doc = book();
    expect(pagesOf(doc, "50%")).toEqual([["percent", 150]]);
    expect(pagesOf(doc, "100%")).toEqual([["percent", 299]]);
    expect(pagesOf(doc, "+5")).toEqual([["relative", 45]]);
    expect(pagesOf(doc, "-100")).toEqual([["relative", 0]]);
    expect(pagesOf(doc, "last")).toEqual([["lastPosition", 77]]);
    expect(pagesOf(doc, "end")).toEqual([["page", 299]]);
    expect(resolveJump(doc, "150%")).toEqual([]);
  });

  it("finds chapters by number", () => {
    const doc = book();
    expect(resolveJump(doc, "ch 2")[0]).toMatchObject({ label: "Chapter 2: Energy", page: 40 });
    expect(resolveJump(doc, "chapter 3")[0].page).toBe(80);
  });

  it("finds headings and bookmarks by name", () => {
    const doc = book();
    expect(pagesOf(doc, "#methods")).toEqual([["outline", 20]]);
    expect(pagesOf(doc, "photo")).toEqual([["outline", 80]]);
    expect(pagesOf(doc, "exam")).toEqual([["bookmark", 150]]);
    expect(pagesOf(doc, "PRÉFACE")).toEqual([["outline", 2]]);
    expect(resolveJump(doc, "zebra")).toEqual([]);
    expect(resolveJump(doc, "")).toEqual([]);
  });

  it("finds word offsets in word documents", () => {
    const doc: JumpDocument = { ...book(), pageCount: 10, pageLabels: [], outline: [], bookmarks: [], wordStarts: [[0, 0], [4, 120], [9, 4000], [15, 6000]] };
    expect(resolveJump(doc, "w 5000")[0].block).toBe(9);
    expect(resolveJump(doc, "word 1")[0].block).toBe(0);
  });
});
