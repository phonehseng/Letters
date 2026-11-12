// tests for the search screen's rules: rows, keys, snippets, sentences and arrival.
import { describe, expect, it } from "vitest";
import { findTextFor, setArrival, takeArrival, type KeyValue } from "./arrival";
import type { DocumentHit, JobStatus, SearchResults } from "./libraryApi";
import {
  countSentence,
  correctionSentence,
  emptyState,
  filtersOf,
  folderOf,
  modifiedAfter,
  passagePlace,
  rowsOf,
  snippetPieces,
  statusSentence,
  stepDocument,
  stepRow,
  targetOf,
  toggleKind,
} from "./results";

function hit(title: string, pages: number[]): DocumentHit {
  return {
    path: `C:\\library\\${title}.pdf`,
    title,
    kind: "pdf",
    modified: 0,
    pages: 10,
    notes: [],
    matchingPassages: pages.length,
    passages: pages.map((page) => ({ number: page, page, heading: "", snippet: `text on ${page}`, highlights: [], score: 1 })),
    score: 1,
  };
}

const results: SearchResults = { documents: [hit("a", [3, 7]), hit("b", [1])], totalDocuments: 2, totalPassages: 3, corrections: [], micros: 100 };

function status(over: Partial<JobStatus>): JobStatus {
  return { phase: "idle", done: 0, total: 0, current: null, documents: 0, passages: 0, bytesOnDisk: 0, locked: 0, needsTextRecognition: 0, unreadable: 0, lastError: null, lastFinished: null, recovered: false, ...over };
}

describe("search results", () => {
  it("become rows: each document, then its pages", () => {
    const rows = rowsOf(results);
    expect(rows).toEqual([
      { kind: "document", document: 0 },
      { kind: "passage", document: 0, passage: 0 },
      { kind: "passage", document: 0, passage: 1 },
      { kind: "document", document: 1 },
      { kind: "passage", document: 1, passage: 0 },
    ]);
    expect(rowsOf(null)).toEqual([]);
  });

  it("the arrows move a row, page keys a document", () => {
    const rows = rowsOf(results);
    expect(stepRow(-1, rows.length, 1)).toBe(0);
    expect(stepRow(-1, rows.length, -1)).toBe(4);
    expect(stepRow(4, rows.length, 1)).toBe(4);
    expect(stepRow(0, 0, 1)).toBe(-1);
    expect(stepDocument(rows, 1, 1)).toBe(3);
    expect(stepDocument(rows, 3, -1)).toBe(0);
    expect(stepDocument(rows, 4, 1)).toBe(4);
  });

  it("a document row opens at its best page", () => {
    expect(targetOf(results, { kind: "document", document: 0 }).passage?.page).toBe(3);
    expect(targetOf(results, { kind: "passage", document: 0, passage: 1 }).passage?.page).toBe(7);
  });

  it("snippets are cut into marked pieces", () => {
    expect(snippetPieces("the cell membrane", [[4, 8], [9, 17]])).toEqual([
      { text: "the ", marked: false },
      { text: "cell", marked: true },
      { text: " ", marked: false },
      { text: "membrane", marked: true },
    ]);
    // broken ranges are skipped rather than garbling the text
    expect(snippetPieces("abc", [[2, 9]])).toEqual([{ text: "abc", marked: false }]);
  });

  it("places, folders and filters", () => {
    expect(passagePlace("pdf", { number: 4, page: 4, heading: "Methods", snippet: "", highlights: [], score: 0 })).toBe("Page 5 · Methods");
    expect(passagePlace("word", { number: 40, page: 2, heading: "", snippet: "", highlights: [], score: 0 })).toBe("About page 3");
    expect(folderOf("C:\\a\\b\\c.pdf")).toBe("C:\\a\\b");
    expect(toggleKind(["pdf"], "pdf")).toEqual([]);
    expect(toggleKind([], "word")).toEqual(["word"]);
    expect(modifiedAfter("any", 1000)).toBeNull();
    expect(modifiedAfter("week", 10 * 86_400_000)).toBe(3 * 86_400_000);
    expect(filtersOf(["pdf"], "any", true, null, 0)).toEqual({ kinds: ["pdf"], modifiedAfter: null, titlesOnly: true, folder: null });
  });
});

describe("what the screen says", () => {
  it("counts, corrections and the job's progress", () => {
    expect(countSentence(results, "cell")).toBe("3 places in 2 documents");
    expect(countSentence({ ...results, totalDocuments: 0 }, "cell")).toBe("No results");
    expect(countSentence(results, " ")).toBe("");
    expect(correctionSentence({ ...results, corrections: [{ typed: "photosynthsis", used: "photosynthesis" }] })).toBe("Showing results for “photosynthesis”.");
    expect(statusSentence(status({ phase: "indexing", done: 131, total: 480 }))).toBe("Indexing 132 of 480 documents");
    expect(statusSentence(status({ phase: "idle", documents: 4800 }))).toBe("4,800 documents indexed");
    expect(statusSentence(status({ phase: "paused" }))).toBe("Indexing is paused.");
  });

  it("empty states offer the next step", () => {
    expect(emptyState("", null, status({}), false)?.detail).toContain("Add a folder to search");
    expect(emptyState("", null, status({ documents: 3 }), true)?.title).toBe("Search All Documents");
    const none = { ...results, documents: [], totalDocuments: 0, totalPassages: 0 };
    expect(emptyState("zebra", none, status({ phase: "indexing", total: 9 }), true)?.title).toBe("No Results Yet");
    expect(emptyState("zebra", none, status({}), true)?.title).toBe("No Results");
    expect(emptyState("cell", results, status({}), true)).toBeNull();
  });
});

describe("arriving at a search result", () => {
  function memory(): KeyValue {
    const map = new Map<string, string>();
    return { getItem: (k) => map.get(k) ?? null, setItem: (k, v) => void map.set(k, v), removeItem: (k) => void map.delete(k) };
  }

  it("is taken once, by the same file in any spelling", () => {
    const store = memory();
    setArrival("C:/Books/Novel.pdf", { page: 41, query: "whale" }, store, 1000);
    expect(takeArrival("c:\\books\\novel.pdf", store, 2000)).toMatchObject({ page: 41, query: "whale" });
    expect(takeArrival("c:\\books\\novel.pdf", store, 2000)).toBeNull();
  });

  it("gives the Find bar plain words", () => {
    expect(findTextFor('"albatross remembered"')).toBe("albatross remembered");
    expect(findTextFor("mitosis OR meiosis -plant")).toBe("mitosis meiosis");
    expect(findTextFor("albatros harbour", [{ typed: "albatros", used: "albatross" }])).toBe("albatross harbour");
  });

  it("goes stale after a minute", () => {
    const store = memory();
    setArrival("a.pdf", { page: 1, query: "x" }, store, 0);
    expect(takeArrival("a.pdf", store, 61_000)).toBeNull();
  });
});
