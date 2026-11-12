import { describe, expect, it } from "vitest";
import { addedSentence, fontFilesAmong } from "./fontFiles";
import { fontKey } from "./fontCommands";
import { downloadNotice } from "./fontTools";
import type { FamilySummary, MissingFont } from "./fontsApi";
import { EMPTY_FILTERS, familyMeta, hasFilters, latestOnly, queryOf, resultSentence, sizeWords, toggle } from "./fontQuery";
import { missingActions, missingSummary, missingTitle } from "./missingFonts";
import { embeddingNote, groupedRows, indexOfFamily, isFamilyRow, matchRows } from "./pickerRows";
import { DEFAULT_SAMPLE, MAX_SAMPLE_CHARS, clampSize, cleanSample, detailSample, sampleFor } from "./sampleText";
import { libraryRowHeight, moveWithKey, rowsPerPage, scrollToShow, visibleRows } from "./virtualList";

function family(name: string, changes: Partial<FamilySummary> = {}): FamilySummary {
  return {
    name,
    category: "sansSerif",
    sources: ["system"],
    onThisComputer: true,
    needsFontFace: false,
    regularFace: `system:${name}#0`,
    styleCount: 4,
    hasBold: true,
    hasItalic: true,
    variable: false,
    monospace: false,
    embeddable: true,
    favourite: false,
    downloadBytes: null,
    ...changes,
  };
}

describe("the virtual list", () => {
  it("draws only the rows on screen and a few around them", () => {
    const rows = visibleRows(1000, 400, 50, 2000, 2);
    expect(rows).toEqual({ start: 18, end: 30, offset: 900, total: 100_000 });
  });

  it("stays inside the list at both ends", () => {
    expect(visibleRows(-50, 400, 50, 3, 6)).toEqual({ start: 0, end: 3, offset: 0, total: 150 });
    expect(visibleRows(99_999_999, 400, 50, 100, 0)).toEqual({ start: 92, end: 100, offset: 4600, total: 5000 });
    expect(visibleRows(0, 400, 50, 0)).toEqual({ start: 0, end: 0, offset: 0, total: 0 });
  });

  it("scrolls as little as possible to show a row", () => {
    expect(scrollToShow(5, 0, 400, 50)).toBe(0);
    expect(scrollToShow(10, 0, 400, 50)).toBe(150);
    expect(scrollToShow(2, 300, 400, 50)).toBe(100);
    expect(rowsPerPage(400, 50)).toBe(8);
    expect(rowsPerPage(10, 50)).toBe(1);
  });

  it("moves with the keys and steps over headings", () => {
    const heading = (index: number) => index === 0 || index === 4;
    const selectable = (index: number) => !heading(index);
    expect(moveWithKey("ArrowDown", -1, 10, 3, selectable)).toBe(1);
    expect(moveWithKey("ArrowDown", 3, 10, 3, selectable)).toBe(5);
    expect(moveWithKey("ArrowUp", 5, 10, 3, selectable)).toBe(3);
    expect(moveWithKey("ArrowUp", 1, 10, 3, selectable)).toBe(1);
    expect(moveWithKey("Home", 7, 10, 3, selectable)).toBe(1);
    expect(moveWithKey("End", 2, 10, 3)).toBe(9);
    expect(moveWithKey("PageDown", 1, 10, 3, selectable)).toBe(5);
    expect(moveWithKey("PageUp", 9, 10, 3)).toBe(6);
    expect(moveWithKey("a", 2, 10, 3)).toBeNull();
    expect(moveWithKey("ArrowDown", 0, 0, 3)).toBeNull();
  });

  it("makes rows tall enough for the preview size", () => {
    expect(libraryRowHeight(12)).toBeLessThan(libraryRowHeight(72));
    expect(libraryRowHeight(28)).toBe(Math.ceil(28 * (4 / 3) * 1.4) + 34);
  });
});

describe("sample text", () => {
  it("is the person's words on one line, or a language's, or the name", () => {
    expect(sampleFor("Inter", "  Hello\n  there  ", null)).toBe("Hello there");
    expect(sampleFor("Inter", "", "vietnamese")).toContain("Đồng ₫");
    expect(sampleFor("Inter", "", null)).toBe("Inter");
    expect(detailSample("", null)).toBe(DEFAULT_SAMPLE);
  });

  it("is cut to one line's worth, counting characters not code units", () => {
    expect([...cleanSample("😀".repeat(500))].length).toBe(MAX_SAMPLE_CHARS);
  });

  it("keeps the size in range", () => {
    expect(clampSize(5)).toBe(12);
    expect(clampSize(200)).toBe(72);
    expect(clampSize(Number.NaN)).toBe(28);
    expect(clampSize(30.4)).toBe(30);
  });
});

describe("the search state", () => {
  it("turns chips into one query", () => {
    expect(queryOf(EMPTY_FILTERS)).toEqual({ text: "" });
    expect(
      queryOf({
        ...EMPTY_FILTERS,
        text: " garmond ",
        category: "serif",
        writing: "greek",
        place: "toDownload",
        embeddable: true,
        favouritesOnly: true,
        mustDrawSample: true,
        sample: "ĐỒNG ₫",
      }),
    ).toEqual({
      text: "garmond",
      categories: ["serif"],
      writing: ["greek"],
      places: ["toDownload"],
      embeddable: true,
      favouritesOnly: true,
      sample: "ĐỒNG ₫",
    });
    // the sample only filters when asked to
    expect(queryOf({ ...EMPTY_FILTERS, sample: "abc" }).sample).toBeUndefined();
  });

  it("toggles chips off when pressed again", () => {
    expect(toggle<string>(null, "serif")).toBe("serif");
    expect(toggle<string>("serif", "serif")).toBeNull();
    expect(toggle<string>("serif", "sansSerif")).toBe("sansSerif");
    expect(hasFilters(EMPTY_FILTERS)).toBe(false);
    expect(hasFilters({ ...EMPTY_FILTERS, embeddable: true })).toBe(true);
  });

  it("says how many fonts were found, and what to try when none were", () => {
    expect(resultSentence(1, EMPTY_FILTERS, false)).toBe("1 font");
    expect(resultSentence(2274, EMPTY_FILTERS, false)).toBe("2,274 fonts");
    expect(resultSentence(300, EMPTY_FILTERS, true)).toBe("300 fonts so far. Still looking for fonts on this computer…");
    expect(resultSentence(0, { ...EMPTY_FILTERS, text: "zzz" }, false)).toBe(
      "No fonts match “zzz”. Check the spelling or try fewer words.",
    );
    expect(resultSentence(0, { ...EMPTY_FILTERS, category: "serif" }, false)).toBe("No fonts match. Try turning off a filter.");
  });

  it("describes a family in a quiet line", () => {
    expect(familyMeta(family("Arial"))).toBe("4 styles · Sans Serif · On This Computer");
    expect(
      familyMeta(family("Lobster", { sources: ["downloadable"], onThisComputer: false, styleCount: 1, category: "display", downloadBytes: 300_000 })),
    ).toBe("1 style · Display · To Download · 293 KB");
    expect(sizeWords(2_500_000)).toBe("2.4 MB");
  });

  it("keeps only the newest answer", async () => {
    let finish: ((value: string) => void)[] = [];
    const slow = latestOnly((text: string) => new Promise<string>((resolve) => finish.push(() => resolve(text))));
    const first = slow("a");
    const second = slow("ab");
    finish[1]("ab");
    finish[0]("a");
    expect(await second).toEqual({ current: true, value: "ab" });
    expect(await first).toEqual({ current: false, value: "a" });
    finish = [];
  });
});

describe("the font picker's rows", () => {
  const families = [family("Arial"), family("Calibri"), family("Carlito", { sources: ["bundled"], needsFontFace: true }), family("Lobster", { onThisComputer: false, sources: ["downloadable"] })];

  it("groups favourites, recent, document fonts and everything", () => {
    const rows = groupedRows(families, { favourites: ["Carlito"], recent: ["Arial", "Lobster"] }, ["Calibri", "Comic Sans MS", "Calibri"]);
    const labels = rows.map((row) => (row.kind === "heading" ? `# ${row.label}` : row.name));
    expect(labels).toEqual([
      "# Favourites",
      "Carlito",
      "# Recent",
      "Arial",
      "# In This Document",
      "Calibri",
      "Comic Sans MS",
      "# All Fonts",
      "Arial",
      "Calibri",
      "Carlito",
    ]);
    // a document font that isn't here is listed without a summary
    const missing = rows.find((row) => isFamilyRow(row) && row.name === "Comic Sans MS");
    expect(missing && isFamilyRow(missing) ? missing.family : "x").toBeNull();
    expect(indexOfFamily(rows, "calibri")).toBe(5);
  });

  it("lists matches on this computer in the engine's order", () => {
    const rows = matchRows(families, [3, 2, 0]);
    expect(rows.map((row) => (isFamilyRow(row) ? row.name : ""))).toEqual(["Carlito", "Arial"]);
  });

  it("notes a font that can't be embedded", () => {
    expect(embeddingNote(family("Arial"))).toBeNull();
    expect(embeddingNote(family("Locked", { embeddable: false }))).toBe(
      "This font’s license doesn’t allow embedding; others will see a similar font unless they have it.",
    );
  });
});

describe("missing fonts", () => {
  const calibri: MissingFont = {
    name: "Calibri",
    downloadable: false,
    substitution: { asked: "Calibri", used: "Carlito", metricCompatible: true, available: true, downloadable: false, sentence: "…" },
  };
  const georgia: MissingFont = {
    name: "Georgia",
    downloadable: false,
    substitution: { asked: "Georgia", used: "Gelasio", metricCompatible: true, available: false, downloadable: true, sentence: "…" },
  };
  const lobster: MissingFont = {
    name: "Lobster",
    downloadable: true,
    substitution: { asked: "Lobster", used: "Arial", metricCompatible: false, available: true, downloadable: false, sentence: "…" },
  };

  it("offers the one action that fixes each", () => {
    expect(missingActions(calibri, true)).toEqual([{ kind: "replace", label: "Replace with Carlito", from: "Calibri", to: "Carlito" }]);
    expect(missingActions(calibri, false)).toEqual([]);
    expect(missingActions(georgia, true)).toEqual([{ kind: "download-substitute", label: "Download Gelasio", family: "Gelasio" }]);
    expect(missingActions(lobster, true)[0]).toEqual({ kind: "download-font", label: "Download Lobster", family: "Lobster" });
  });

  it("counts and explains them", () => {
    expect(missingTitle(0)).toBe("No Missing Fonts");
    expect(missingTitle(1)).toBe("1 Missing Font");
    expect(missingTitle(3)).toBe("3 Missing Fonts");
    expect(missingSummary([calibri, georgia])).toContain("same letter widths");
    expect(missingSummary([calibri, lobster])).toContain("similar font");
  });
});

describe("files, keys and words", () => {
  it("picks font files out of a drop", () => {
    expect(fontFilesAmong(["a.TTF", "b.otf", "c.pdf", "d.woff2", "e.woff", "f.ttc", "notes.txt"])).toEqual(["a.TTF", "b.otf", "d.woff2", "e.woff", "f.ttc"]);
  });

  it("says what adding did", () => {
    expect(addedSentence({ added: [1, 2], rejected: [], alreadyAdded: [] })).toBe("Added 2 fonts.");
    expect(addedSentence({ added: [], rejected: [{ file: "x.ttf", reason: "“x.ttf” is damaged." }], alreadyAdded: ["y.ttf"] })).toBe(
      "1 font was already added. “x.ttf” is damaged.",
    );
    expect(addedSentence({ added: [], rejected: [], alreadyAdded: [] })).toBe("No fonts were found.");
  });

  it("opens the library with ctrl or command T only", () => {
    expect(fontKey({ key: "t", ctrlKey: true, metaKey: false, altKey: false })).toBe("font-library");
    expect(fontKey({ key: "T", ctrlKey: false, metaKey: true, altKey: false })).toBe("font-library");
    expect(fontKey({ key: "t", ctrlKey: true, metaKey: false, altKey: false, shiftKey: true })).toBeNull();
    expect(fontKey({ key: "t", ctrlKey: true, metaKey: false, altKey: true })).toBeNull();
  });

  it("says plainly what a download sends", () => {
    expect(downloadNotice("raw.githubusercontent.com")).toContain("Only the request for that font is sent");
  });
});
