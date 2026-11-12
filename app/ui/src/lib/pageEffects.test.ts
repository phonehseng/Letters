import { describe, expect, it } from "vitest";
import {
  cropEffect,
  deleteEffect,
  extractEffect,
  fillTokens,
  headerFooterEffect,
  insertBlankEffect,
  insertFromEffect,
  insertPlace,
  megabytes,
  mergeEffect,
  pageNumberLabel,
  pageNumbersEffect,
  removeEffect,
  rotateEffect,
  splitEffect,
  targetHint,
  watermarkEffect,
} from "./pageEffects";
import type { PageNumberSettings } from "./pageApi";

const numbers = (extra: Partial<PageNumberSettings> = {}): PageNumberSettings => ({
  position: { vertical: "bottom", horizontal: "center" },
  startNumber: 1,
  format: "ofTotal",
  skipFirstPage: false,
  pages: null,
  ...extra,
});

describe("what will happen", () => {
  it("describes page numbers the way the coordinator's example reads", () => {
    expect(pageNumbersEffect(numbers({ format: "pageWord" }), 30)).toBe(
      "Adds “Page 1” at the bottom centre of 30 pages. Undo with Ctrl Z.",
    );
    expect(pageNumbersEffect(numbers(), 30)).toBe("Adds “1 of 30” at the bottom centre of 30 pages. Undo with Ctrl Z.");
    expect(pageNumbersEffect(numbers({ skipFirstPage: true, format: "number" }), 30)).toBe(
      "Adds “2” at the bottom centre of 29 pages, leaving the first page without a number. Undo with Ctrl Z.",
    );
    expect(pageNumbersEffect(numbers({ skipFirstPage: true, pages: [0] }), 30)).toMatch(/^Choose more than one page/);
  });

  it("labels page numbers as core/pages does", () => {
    expect(pageNumberLabel(4, 9, "roman")).toBe("iv");
    expect(pageNumberLabel(3, 9, "number")).toBe("3");
  });

  it("names the turn and the pages", () => {
    expect(rotateEffect(1, [2], 10)).toBe("Turns page 3 a quarter turn to the right. Undo with Ctrl Z.");
    expect(rotateEffect(-1, [0, 1], 10)).toBe("Turns pages 1–2 a quarter turn to the left. Undo with Ctrl Z.");
    expect(rotateEffect(2, [0, 1, 2], 3)).toBe("Turns all 3 pages upside down. Undo with Ctrl Z.");
  });

  it("refuses to describe deleting every page", () => {
    expect(deleteEffect([1], 3)).toEqual({ text: "Takes page 2 out of this document. Undo with Ctrl Z.", blocked: false });
    expect(deleteEffect([0, 1, 2], 3).blocked).toBe(true);
  });

  it("says where inserted pages go", () => {
    expect(insertPlace(0, 5)).toBe("before page 1");
    expect(insertPlace(3, 5)).toBe("after page 3");
    expect(insertPlace(5, 5)).toBe("at the end");
    expect(insertBlankEffect(1, 3, 5)).toBe("Adds 1 blank page after page 3. Undo with Ctrl Z.");
    expect(insertFromEffect(null, null, 3, 5)).toBe("Choose a PDF to copy pages from.");
    expect(insertFromEffect({ name: "cover.pdf", pageCount: 2 }, null, 0, 5)).toBe(
      "Copies all 2 pages of “cover.pdf” in before page 1. “cover.pdf” doesn’t change. Undo with Ctrl Z.",
    );
  });

  it("says that tools making new files leave this document alone", () => {
    expect(extractEffect([0, 1, 2], 10)).toBe("Saves a copy of pages 1–3 as a new PDF. This document doesn’t change.");
    expect(splitEffect({ mode: "everyNPages", pagesPerPart: 10 }, 35)).toBe(
      "Makes 4 new PDFs of up to 10 pages each in the folder you choose. Files already there are never replaced. This document doesn’t change.",
    );
    expect(splitEffect({ mode: "byRanges", ranges: [{ start: 0, end: 2 }] }, 35)).toMatch(/^Makes 1 new PDF, one for each range, in the folder you choose/);
    expect(splitEffect({ mode: "byBookmarks" }, 35)).toMatch(/^Makes a new PDF for each chapter in the outline, in the folder you choose/);
    expect(splitEffect({ mode: "byMaxSize", maxBytes: 5 * 1024 * 1024 }, 35)).toMatch(/of up to 5 MB each/);
    expect(megabytes(1.5 * 1024 * 1024)).toBe("1.5 MB");
    expect(mergeEffect(1)).toBe("Add at least two PDFs to merge.");
    expect(mergeEffect(3)).toMatch(/^Makes one new PDF from 3 PDFs/);
  });

  it("is precise about crop hiding, not removing", () => {
    expect(cropEffect([4], 10)).toBe(
      "Hides everything outside the rectangle on page 5. The hidden part stays in the file. Undo with Ctrl Z.",
    );
  });

  it("shows a sample of the header or footer", () => {
    const settings = {
      edge: "top" as const,
      left: "{file}",
      center: "",
      right: "Page {page} of {pages}",
      pages: null,
      fileName: "essay.pdf",
      date: "11 November 2026",
    };
    expect(headerFooterEffect(settings, 30)).toBe(
      "Adds a header to 30 pages, starting “essay.pdf  Page 1 of 30”. Undo with Ctrl Z.",
    );
    expect(headerFooterEffect({ ...settings, left: "", right: " " }, 30)).toBe("Type some text for the header first.");
    expect(fillTokens("{date} – {x} {page}", 2, 9, "today", "f")).toBe("today – {x} 2");
  });

  it("describes watermarks and asks for what is missing", () => {
    expect(watermarkEffect({ kind: "text", text: "Draft" }, "inFront", 0.3, null, 12)).toBe(
      "Adds “Draft” across the middle of 12 pages, in front of the page, at 30% opacity. Undo with Ctrl Z.",
    );
    expect(watermarkEffect({ kind: "text", text: " " }, "inFront", 0.3, null, 12)).toBe("Type the watermark text first.");
    expect(watermarkEffect({ kind: "picture", name: null }, "behind", 0.3, null, 12)).toBe(
      "Choose a PNG or JPEG picture first.",
    );
  });

  it("says which pages a tool works on when nothing is selected", () => {
    expect(targetHint({ pages: [2], fromSelection: false }, 9)).toBe(
      "Works on the page you are reading, page 3. To choose other pages, select them in the sidebar.",
    );
    expect(targetHint({ pages: [1, 2], fromSelection: true }, 9)).toBe("Works on the selected pages: pages 2–3.");
  });

  it("describes removing only what Letters added", () => {
    expect(removeEffect("pageNumbers", null, 12)).toBe(
      "Takes the page numbers that Letters added off all 12 pages. Nothing else on the pages changes. Undo with Ctrl Z.",
    );
  });
});
