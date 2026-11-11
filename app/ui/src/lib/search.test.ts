import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchMatch, SearchResult } from "./api";
import {
  createSearchRunner,
  firstMatchFrom,
  highlightsOnPage,
  matchBounds,
  matchCountLabel,
  matchesByPage,
  nextMatch,
  previousMatch,
  SEARCH_DELAY_MS,
} from "./search";

const match = (page: number, x = 10, y = 20): SearchMatch => ({
  page,
  rects: [{ x, y, width: 30, height: 12 }],
  snippet: "…the word…",
});

describe("moving between matches", () => {
  it("goes round in both directions", () => {
    expect(nextMatch(0, 3)).toBe(1);
    expect(nextMatch(2, 3)).toBe(0);
    expect(previousMatch(0, 3)).toBe(2);
    expect(previousMatch(2, 3)).toBe(1);
  });

  it("has nowhere to go without matches", () => {
    expect(nextMatch(0, 0)).toBe(-1);
    expect(previousMatch(0, 0)).toBe(-1);
  });

  it("starts on the first match at or after the page being read", () => {
    const matches = [match(1), match(4), match(4), match(9)];
    expect(firstMatchFrom(matches, 4)).toBe(1);
    expect(firstMatchFrom(matches, 5)).toBe(3);
    expect(firstMatchFrom(matches, 0)).toBe(0);
    // past the last match it goes round to the first
    expect(firstMatchFrom(matches, 20)).toBe(0);
    expect(firstMatchFrom([], 3)).toBe(-1);
  });
});

describe("matchCountLabel", () => {
  const result = (count: number, truncated = false): SearchResult => ({
    matches: Array.from({ length: count }, () => match(0)),
    truncated,
  });

  it("counts from one", () => {
    expect(matchCountLabel(2, result(41))).toBe("3 of 41");
  });

  it("says when there are more than the limit", () => {
    expect(matchCountLabel(0, result(500, true))).toBe("1 of 500+");
  });

  it("says when nothing was found, and nothing before a search", () => {
    expect(matchCountLabel(-1, result(0))).toBe("Not found");
    expect(matchCountLabel(-1, null)).toBe("");
  });
});

describe("highlights", () => {
  const matches = [match(0), match(2, 100, 200), match(2, 50, 60)];
  const byPage = matchesByPage(matches);

  it("groups matches by page", () => {
    expect(byPage.get(2)).toEqual([1, 2]);
    expect(byPage.get(1)).toBeUndefined();
  });

  it("scales point rectangles by the zoom and marks the current one", () => {
    const highlights = highlightsOnPage(matches, byPage, 2, 1.5, 2);
    expect(highlights).toEqual([
      { key: "1/0", left: 150, top: 300, width: 45, height: 18, current: false },
      { key: "2/0", left: 75, top: 90, width: 45, height: 18, current: true },
    ]);
  });

  it("has nothing for a page without matches", () => {
    expect(highlightsOnPage(matches, byPage, 1, 1, 0)).toEqual([]);
  });

  it("finds the box around a match that wraps onto two lines", () => {
    const wrapped: SearchMatch = {
      page: 0,
      rects: [
        { x: 300, y: 100, width: 80, height: 12 },
        { x: 72, y: 114, width: 40, height: 12 },
      ],
      snippet: "",
    };
    expect(matchBounds(wrapped)).toEqual({ x: 72, y: 100, width: 308, height: 26 });
    expect(matchBounds({ page: 0, rects: [], snippet: "" })).toBeNull();
  });
});

describe("createSearchRunner", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  /** a fake engine whose answers arrive whenever the test says so */
  function fakeEngine() {
    const pending = new Map<string, (result: SearchResult) => void>();
    const search = vi.fn(
      (query: string) => new Promise<SearchResult>((resolve) => pending.set(query, resolve)),
    );
    const answer = async (query: string, count: number) => {
      pending.get(query)?.({ matches: Array.from({ length: count }, () => match(0)), truncated: false });
      await vi.runAllTimersAsync();
    };
    return { search, answer };
  }

  it("waits for a pause in typing before searching", async () => {
    const engine = fakeEngine();
    const runner = createSearchRunner({ search: engine.search, onResult: () => {}, onError: () => {} });
    runner.update("t");
    runner.update("th");
    runner.update("the");
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS - 1);
    expect(engine.search).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(engine.search).toHaveBeenCalledTimes(1);
    expect(engine.search).toHaveBeenCalledWith("the");
  });

  it("only shows the answer to the latest query", async () => {
    const engine = fakeEngine();
    const shown: string[] = [];
    const runner = createSearchRunner({
      search: engine.search,
      onResult: (query) => shown.push(query),
      onError: () => {},
    });
    runner.update("th");
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS);
    runner.update("the");
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS);
    // the newer search answers first, then the older one arrives late
    await engine.answer("the", 3);
    await engine.answer("th", 9);
    expect(shown).toEqual(["the"]);
  });

  it("clears at once for an empty query and ignores answers after cancel", async () => {
    const engine = fakeEngine();
    const results: (SearchResult | null)[] = [];
    const runner = createSearchRunner({
      search: engine.search,
      onResult: (_query, result) => results.push(result),
      onError: () => {},
    });
    runner.update("word");
    await vi.advanceTimersByTimeAsync(SEARCH_DELAY_MS);
    runner.cancel();
    await engine.answer("word", 2);
    expect(results).toEqual([]);
    runner.update("  ");
    expect(results).toEqual([null]);
  });

  it("reports a failure only for the latest query", async () => {
    const errors: string[] = [];
    const runner = createSearchRunner({
      search: (query) => Promise.reject(new Error(query)),
      onResult: () => {},
      onError: (query) => errors.push(query),
    });
    runner.update("broken");
    await vi.runAllTimersAsync();
    expect(errors).toEqual(["broken"]);
  });
});
