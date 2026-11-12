import { describe, expect, it } from "vitest";
import { describePages, formatPages, pageCountText, pagesAsTypedText, parsePageRanges } from "./pageRanges";

describe("parsePageRanges", () => {
  it("reads pages, runs, open ends and mixed separators", () => {
    const result = parsePageRanges("1-3, 7 10-", 12);
    expect(result).toEqual({
      ok: true,
      pages: [0, 1, 2, 6, 9, 10, 11],
      spans: [
        { start: 0, end: 2 },
        { start: 6, end: 6 },
        { start: 9, end: 11 },
      ],
    });
  });

  it("accepts a run from the start, en dashes and spaces around dashes", () => {
    expect(parsePageRanges("-2", 5)).toMatchObject({ ok: true, pages: [0, 1] });
    expect(parsePageRanges("2 – 4", 5)).toMatchObject({ ok: true, pages: [1, 2, 3] });
  });

  it("keeps overlapping runs as typed but lists each page once", () => {
    const result = parsePageRanges("1-3, 2-4", 5);
    expect(result).toMatchObject({ ok: true, pages: [0, 1, 2, 3] });
    if (result.ok) expect(result.spans).toHaveLength(2);
  });

  it("explains every mistake in a plain sentence", () => {
    expect(parsePageRanges("  ", 5)).toEqual({ ok: false, error: "Type the pages, such as 1-3, 7." });
    expect(parsePageRanges("1-3, x", 5)).toEqual({
      ok: false,
      error: "“x” isn’t a page number. Use numbers, commas and dashes, such as 1-3, 7.",
    });
    expect(parsePageRanges("12", 10)).toEqual({ ok: false, error: "Page 12 doesn’t exist. This document has 10 pages." });
    expect(parsePageRanges("0-2", 10)).toEqual({ ok: false, error: "Pages are counted from 1." });
    expect(parsePageRanges("5-3", 10)).toEqual({
      ok: false,
      error: "“5-3” goes backwards. Write the lower page first, such as 3-5.",
    });
    expect(parsePageRanges("-", 10)).toMatchObject({ ok: false });
    expect(parsePageRanges("2", 1)).toEqual({ ok: false, error: "Page 2 doesn’t exist. This document has 1 page." });
  });
});

describe("writing pages back as text", () => {
  it("joins runs with dashes", () => {
    expect(formatPages([0, 1, 2, 6, 9])).toBe("1–3, 7, 10");
    expect(pagesAsTypedText([2, 0, 1])).toBe("1-3");
  });

  it("describes pages in a sentence", () => {
    expect(describePages([2], 10)).toBe("page 3");
    expect(describePages([0, 1, 2, 6], 10)).toBe("pages 1–3 and 7");
    expect(describePages([0, 4, 6], 10)).toBe("pages 1, 5 and 7");
    expect(describePages([0, 1, 2], 3)).toBe("all 3 pages");
    expect(describePages([0], 1)).toBe("page 1");
    expect(pageCountText(1)).toBe("1 page");
  });
});
