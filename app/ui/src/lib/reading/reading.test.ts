// tests for back and forward, reading progress and the page timer.
import { describe, expect, it } from "vitest";
import { EMPTY_HISTORY, goBack, goForward, jumped, MAX_PLACES } from "./history";
import { PageTimer, pageOfTotal, percentThrough, progressSentence, timeLeft } from "./progress";

describe("back and forward", () => {
  it("returns from a jump and goes forward again", () => {
    let history = jumped(EMPTY_HISTORY, { page: 4 }, { page: 250 });
    const back = goBack(history, { page: 251 })!;
    expect(back.to).toEqual({ page: 4 });
    history = back.history;
    const forward = goForward(history, { page: 4 })!;
    expect(forward.to).toEqual({ page: 251 });
    expect(goForward(forward.history, { page: 251 })).toBeNull();
  });

  it("a new jump clears forward, and a jump to the same place is not one", () => {
    let history = jumped(EMPTY_HISTORY, { page: 1 }, { page: 9 });
    history = goBack(history, { page: 9 })!.history;
    history = jumped(history, { page: 1 }, { page: 30 });
    expect(history.forward).toEqual([]);
    expect(jumped(history, { page: 3 }, { page: 3 })).toBe(history);
    expect(goBack(EMPTY_HISTORY, { page: 0 })).toBeNull();
  });

  it("forgets the oldest places", () => {
    let history = EMPTY_HISTORY;
    for (let i = 0; i < MAX_PLACES + 20; i++) history = jumped(history, { page: i * 2 }, { page: i * 2 + 1 });
    expect(history.back.length).toBe(MAX_PLACES);
    expect(history.back[0].page).toBe(40);
  });
});

describe("reading progress", () => {
  it("says where the reader is", () => {
    expect(pageOfTotal(40, 300)).toBe("Page 41 of 300");
    expect(pageOfTotal(10, 300, "xi")).toBe("Page 11 (xi) of 300");
    expect(percentThrough(0, 300)).toBe(0);
    expect(percentThrough(299, 300)).toBe(100);
    expect(percentThrough(0, 1)).toBe(100);
  });

  it("estimates time left from the reader's own speed", () => {
    expect(timeLeft(0, 300, null)).toBe("");
    expect(timeLeft(289, 300, 60)).toBe("About 10 minutes left");
    expect(timeLeft(0, 300, 60)).toBe("About 5 hours left");
    expect(timeLeft(298, 300, 20)).toBe("Less than a minute left");
    expect(timeLeft(299, 300, 60)).toBe("");
    expect(progressSentence(40, 300, null)).toBe("Page 41 of 300 · 13%");
  });

  it("measures only pages read through in order", () => {
    let now = 0;
    const timer = new PageTimer(() => now);
    expect(timer.moved(4)).toBeNull();
    now = 45_000;
    expect(timer.moved(5)).toBe(45);
    now = 50_000;
    // a jump is not reading
    expect(timer.moved(90)).toBeNull();
    now = 60_000;
    timer.restart();
    now = 70_000;
    expect(timer.moved(91)).toBe(10);
  });
});
