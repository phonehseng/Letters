// tests for the Compare screen's pure modules: grouping and stepping
// through changes, the scroll map, the virtual list, the overlay, progress
// and choosing the two versions. no window is needed.
import { describe, expect, it } from "vitest";
import type { Change, ComparisonResult, PagePair, Summary } from "./compareApi";
import {
  changeAnnouncement,
  findChange,
  ignoredLine,
  listRows,
  markStyle,
  orderedChanges,
  pairHeading,
  positionText,
  rectsOn,
  stepChange,
  summaryLine,
} from "./changes";
import { rowTops, scrollToRow, visibleRows } from "./changeList";
import { flashMode, flashStep, layersAt, pictureScale, sliderText } from "./overlay";
import { progressFraction, progressText } from "./progress";
import { columnLayout, followingScrollTop, invertMap, mapScroll, matchingHeight, overlaySizesOf, scrollTopToShow, slotOfPage, slotsOf } from "./scrollSync";
import { EMPTY_SETUP, isReady, setupEffect, setupFromDocument, swap, withDropped } from "./setup";
import { reportOptions, suggestedReportPath } from "./report";

describe("the report", () => {
  it("is suggested beside the newer file", () => {
    expect(suggestedReportPath("C:\\work\\essay v2.pdf", "pdf")).toBe("C:\\work\\essay v2 - changes.pdf");
    expect(suggestedReportPath(null, "text")).toBe("Changes.txt");
  });

  it("names both versions and the day it was made", () => {
    const words = reportOptions("a.pdf", "b.pdf", false, new Date(2026, 10, 12), "");
    expect(words).toMatchObject({ oldName: "a.pdf", newName: "b.pdf", thumbnails: false, language: "en" });
    expect(words.madeOn).toContain("2026");
  });

  it("gives the overlay room for the bigger page of each pair", () => {
    const wide = { ...pairOf(), leftSize: { width: 612, height: 792 }, rightSize: { width: 792, height: 612 } };
    expect(overlaySizesOf([wide])).toEqual([{ width: 792, height: 792 }]);
  });
});

/** a bare page pair, for the report tests above the main fixtures. */
function pairOf(): PagePair {
  return { leftPage: 0, rightPage: 0, leftSize: null, rightSize: null, similarity: 1, changes: [], scrollMap: [], offset: { dx: 0, dy: 0 }, notes: [] };
}

const LETTER = { width: 612, height: 792 };

function change(id: number, kind: Change["kind"], category: Change["category"], pages: [number | null, number | null] = [0, 0]): Change {
  return {
    id,
    kind,
    category,
    leftPage: pages[0],
    rightPage: pages[1],
    leftRects: [{ x: 72, y: 100, width: 40, height: 12 }],
    rightRects: [{ x: 72, y: 130, width: 40, height: 12 }],
    oldText: null,
    newText: null,
    detail: `Change number ${id}`,
  };
}

function pair(left: number | null, right: number | null, changes: Change[], scrollMap: PagePair["scrollMap"] = []): PagePair {
  return {
    leftPage: left,
    rightPage: right,
    leftSize: left === null ? null : LETTER,
    rightSize: right === null ? null : LETTER,
    similarity: 1,
    changes,
    scrollMap,
    offset: { dx: 0, dy: 0 },
    notes: [],
  };
}

const summary = (counts: Partial<Summary>): Summary => ({
  text: 0,
  formatting: 0,
  layout: 0,
  image: 0,
  page: 0,
  total: 0,
  ignored: 0,
  pagePairs: 0,
  pagePairsWithChanges: 0,
  ...counts,
});

const result: ComparisonResult = {
  leftPageCount: 3,
  rightPageCount: 4,
  pagePairs: [
    pair(0, 0, [change(1, "replaced", "text"), change(2, "restyled", "formatting")]),
    pair(1, 1, []),
    pair(null, 2, [change(3, "pageAdded", "page", [null, 2])]),
    pair(2, 3, [change(4, "moved", "text", [0, 3]), change(5, "shifted", "layout", [2, 3])]),
  ],
  summary: summary({ text: 2, formatting: 1, layout: 1, page: 1, total: 5 }),
  notes: [],
};

const ALL = new Set<Change["category"]>(["text", "formatting", "layout", "image", "page"]);

describe("grouping and filtering changes", () => {
  it("lists each page pair with changes under a heading", () => {
    const rows = listRows(result, ALL);
    expect(rows.map((row) => (row.type === "heading" ? row.text : row.change.id))).toEqual([
      "Page 1",
      1,
      2,
      "New page 3",
      3,
      "Old page 3, new page 4",
      4,
      5,
    ]);
  });

  it("leaves out categories that are filtered away, and their empty headings", () => {
    const rows = listRows(result, new Set(["text"]));
    expect(rows.map((row) => (row.type === "heading" ? `${row.text} (${row.count})` : row.change.id))).toEqual([
      "Page 1 (1)",
      1,
      "Old page 3, new page 4 (1)",
      4,
    ]);
  });

  it("words page headings plainly", () => {
    expect(pairHeading(pair(4, null, []))).toBe("Old page 5");
    expect(pairHeading(pair(1, 1, []))).toBe("Page 2");
  });
});

describe("stepping through changes", () => {
  const ordered = orderedChanges(result, ALL);

  it("goes to the first or last change when none is selected", () => {
    expect(stepChange(ordered, null, "next")).toBe(1);
    expect(stepChange(ordered, null, "previous")).toBe(5);
  });

  it("moves one change at a time and stays at the ends", () => {
    expect(stepChange(ordered, 2, "next")).toBe(3);
    expect(stepChange(ordered, 2, "previous")).toBe(1);
    expect(stepChange(ordered, 5, "next")).toBe(5);
    expect(stepChange(ordered, 1, "previous")).toBe(1);
  });

  it("skips changes that are filtered away", () => {
    const textOnly = orderedChanges(result, new Set(["text"]));
    expect(stepChange(textOnly, 1, "next")).toBe(4);
    expect(stepChange([], null, "next")).toBeNull();
  });

  it("says where the selection is", () => {
    expect(positionText(ordered, 3)).toBe("Change 3 of 5");
    expect(positionText(ordered, 99)).toBe("");
    const found = findChange(result, 4)!;
    expect(found.pair).toBe(3);
    expect(changeAnnouncement(ordered, found.change, result.pagePairs[3])).toBe(
      "Change 4 of 5. Old page 3, new page 4. Moved: Change number 4",
    );
    expect(findChange(result, null)).toBeNull();
  });

  it("finds a moved passage's rectangles on the page it came from", () => {
    const moved = findChange(result, 4)!.change;
    expect(rectsOn(moved, "old", 0)).toHaveLength(1);
    expect(rectsOn(moved, "old", 2)).toHaveLength(0);
    expect(rectsOn(moved, "new", 3)).toHaveLength(1);
  });
});

describe("words and marks", () => {
  it("sums up the changes", () => {
    expect(summaryLine(summary({ total: 14, text: 9, formatting: 3, layout: 2 }))).toBe("14 changes: 9 text, 3 formatting, 2 layout");
    expect(summaryLine(summary({ total: 1, image: 1 }))).toBe("1 change: 1 pictures");
    expect(summaryLine(summary({ total: 0, ignored: 3 }))).toBe("These two documents are the same.");
  });

  it("says what was left out on purpose", () => {
    expect(ignoredLine(summary({ ignored: 0 }))).toBeNull();
    expect(ignoredLine(summary({ ignored: 1 }))).toBe("1 difference only in spacing or hyphenation was left out.");
  });

  it("gives every kind a tone and a word, never colour alone", () => {
    expect(markStyle("replaced", "old")).toEqual({ tone: "removed", label: "Changed" });
    expect(markStyle("replaced", "new")).toEqual({ tone: "added", label: "Changed" });
    expect(markStyle("deleted", "old").tone).toBe("removed");
    expect(markStyle("restyled", "new").tone).toBe("formatting");
    expect(markStyle("reflowed", "new")).toEqual({ tone: "layout", label: "Layout" });
    expect(markStyle("redrawn", "old").tone).toBe("picture");
    expect(markStyle("pageMoved", "old").label).toBe("Page moved");
  });
});

describe("keeping the two columns in step", () => {
  const map = [
    { leftY: 0, rightY: 0 },
    { leftY: 100, rightY: 120 },
    { leftY: 792, rightY: 792 },
  ];

  it("maps heights the way the engine does", () => {
    // the same cases as map_scroll's test in core/compare/src/result.rs
    expect(mapScroll(map, 50)).toBe(60);
    expect(mapScroll(map, 100)).toBe(120);
    expect(Math.abs(mapScroll(map, 446) - 456)).toBeLessThan(0.01);
    expect(mapScroll([], 33)).toBe(33);
  });

  it("maps back from the new side", () => {
    expect(invertMap(map)[1]).toEqual({ leftY: 120, rightY: 100 });
    expect(mapScroll(invertMap(map), 60)).toBe(50);
  });

  it("uses the same share of the page when one side has no page", () => {
    expect(matchingHeight(pair(null, 2, []), "new", 396, LETTER, LETTER)).toBe(396);
    expect(matchingHeight(pair(0, 0, [], map), "old", 50, LETTER, LETTER)).toBe(60);
  });

  it("lines up slots by pair, with placeholders for missing pages", () => {
    const oldSlots = slotsOf(result.pagePairs, "old");
    expect(oldSlots.map((slot) => slot.page)).toEqual([0, 1, null, 2]);
    expect(oldSlots[2].size).toEqual(LETTER);
    expect(slotOfPage(oldSlots, 2)).toBe(3);
    expect(slotOfPage(oldSlots, null)).toBe(-1);
  });

  it("scrolls the other column to the matching height", () => {
    const pairs = [pair(0, 0, [], map), pair(1, 1, [])];
    const oldSlots = slotsOf(pairs, "old");
    const newSlots = slotsOf(pairs, "new");
    const zoom = 1;
    const left = columnLayout(oldSlots, zoom, 0);
    const right = columnLayout(newSlots, zoom, 0);
    // 50 points down the first old page, just under a 0px toolbar
    const top = left.pages[0].top + 50;
    expect(followingScrollTop(pairs, "old", left, right, oldSlots, newSlots, top, 0, zoom)).toBeCloseTo(right.pages[0].top + 60);
    expect(followingScrollTop(pairs, "new", right, left, newSlots, oldSlots, right.pages[0].top + 60, 0, zoom)).toBeCloseTo(left.pages[0].top + 50);
  });

  it("puts a revealed change a third of the way down the view", () => {
    const layout = columnLayout(slotsOf([pair(0, 0, [])], "old"), 2, 50);
    expect(scrollTopToShow(layout, 0, 300, 2, 50, 650)).toBe(layout.pages[0].top + 600 - 50 - 200);
    expect(scrollTopToShow(layout, 5, 300, 2, 50, 650)).toBe(0);
  });
});

describe("the virtual changes list", () => {
  const rows = listRows(result, ALL);

  it("places rows by their fixed heights", () => {
    const { tops, total } = rowTops(rows, 10);
    expect(tops.slice(0, 3)).toEqual([0, 20, 55]);
    expect(total).toBe(3 * 20 + 5 * 35);
  });

  it("makes only the rows in view", () => {
    const { tops } = rowTops(rows, 10);
    const shown = visibleRows(rows, tops, 10, 60, 40, 0);
    expect(shown.map((spot) => spot.index)).toEqual([2, 3]);
    expect(visibleRows([], [], 10, 0, 100)).toEqual([]);
  });

  it("scrolls a row into view only when needed", () => {
    expect(scrollToRow(100, 35, 0, 300)).toBeNull();
    expect(scrollToRow(400, 35, 0, 300)).toBe(135);
    expect(scrollToRow(10, 35, 50, 300)).toBe(10);
  });
});

describe("the overlay view", () => {
  it("fades from old through the overlay to new", () => {
    expect(layersAt(0)).toEqual({ old: 1, overlay: 1, next: 0 });
    expect(layersAt(50)).toEqual({ old: 0, overlay: 1, next: 0 });
    expect(layersAt(100)).toEqual({ old: 0, overlay: 1, next: 1 });
    expect(layersAt(75).next).toBeCloseTo(0.5);
    expect(layersAt(-20).old).toBe(1);
  });

  it("names the slider's position", () => {
    expect(sliderText(0)).toBe("Old version");
    expect(sliderText(50)).toBe("Both versions, changes in colour");
    expect(sliderText(80)).toBe("Mostly the new version (80%)");
  });

  it("flashes between the versions, and never by itself with reduced motion", () => {
    expect(flashStep(50)).toBe(0);
    expect(flashStep(0)).toBe(100);
    expect(flashMode(true)).toBe("step");
    expect(flashMode(false)).toBe("auto");
  });

  it("asks for sharp but bounded pictures", () => {
    expect(pictureScale(1, 1.5)).toBe(1.5);
    expect(pictureScale(4, 2)).toBe(3);
    expect(pictureScale(0.05, 1)).toBe(0.25);
  });
});

describe("progress", () => {
  it("fills the bar by stage", () => {
    expect(progressFraction({ state: "running", stage: null, done: 0, total: 0 })).toBe(0);
    expect(progressFraction({ state: "running", stage: "readingNew", done: 5, total: 10 })).toBeCloseTo(0.225);
    expect(progressFraction({ state: "running", stage: "comparingPages", done: 250, total: 500 })).toBeCloseTo(0.65);
    expect(progressFraction({ state: "finished", summary: summary({}) })).toBe(1);
  });

  it("says what it is doing", () => {
    expect(progressText({ state: "running", stage: "comparingPages", done: 120, total: 500 })).toBe("Comparing pages: 120 of 500");
    expect(progressText({ state: "running", stage: null, done: 0, total: 0 })).toBe("Starting…");
  });
});

describe("choosing what to compare", () => {
  it("starts from the open document as the newer version", () => {
    const setup = setupFromDocument({ id: 7, name: "essay.pdf" });
    expect(setup.newer?.input).toEqual({ kind: "open", id: 7 });
    expect(isReady(setup)).toBe(false);
    expect(setupEffect(setup)).toBe("Choose the older version to compare with.");
  });

  it("swaps the two versions", () => {
    const setup = withDropped(setupFromDocument({ id: 7, name: "essay.pdf" }), ["C:\\drafts\\essay v1.pdf"]);
    expect(setup.older?.name).toBe("essay v1.pdf");
    const swapped = swap(setup);
    expect(swapped.older?.name).toBe("essay.pdf");
    expect(swapped.newer?.name).toBe("essay v1.pdf");
  });

  it("takes two dropped pdfs as older and newer, and ignores other files", () => {
    const setup = withDropped(EMPTY_SETUP, ["C:\\a.pdf", "C:\\notes.txt", "C:\\b.PDF"]);
    expect(setup.older?.name).toBe("a.pdf");
    expect(setup.newer?.name).toBe("b.PDF");
    expect(isReady(setup)).toBe(true);
    expect(setupEffect(setup)).toBe("Finds what changed from “a.pdf” to “b.PDF”. Neither file changes.");
    expect(withDropped(EMPTY_SETUP, ["C:\\notes.txt"])).toEqual(EMPTY_SETUP);
  });
});
