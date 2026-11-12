// tests for writing goals, the streak and the manuscript navigator's counts.
import { describe, expect, it } from "vitest";
import { clock, dayKey, goalLine, parseGoal, sessionLine, share, streak, wordsAdded } from "./goals";
import { countWords, moveSection, sectionCounts, wordsLabel, type Heading } from "./manuscript";

describe("writing goals", () => {
  it("counts a streak of days that reached the goal", () => {
    const days = { "2026-11-09": 600, "2026-11-10": 500, "2026-11-11": 520, "2026-11-12": 100 };
    // today is not there yet: the streak still counts up to yesterday
    expect(streak(days, 500, "2026-11-12")).toBe(3);
    expect(streak({ ...days, "2026-11-12": 700 }, 500, "2026-11-12")).toBe(4);
    expect(streak(days, 500, "2026-11-14")).toBe(0);
    // across the end of a month
    expect(streak({ "2026-10-31": 10, "2026-11-01": 10 }, 5, "2026-11-01")).toBe(2);
  });

  it("days are local dates", () => {
    expect(dayKey(new Date(2026, 0, 5, 23, 30))).toBe("2026-01-05");
  });

  it("deleting words never counts against the day", () => {
    expect(wordsAdded(100, 140)).toBe(40);
    expect(wordsAdded(140, 100)).toBe(0);
  });

  it("writes the quiet lines and reads typed goals", () => {
    expect(goalLine(1240, 3000)).toBe("1,240 of 3,000 words");
    expect(goalLine(1240, null)).toBe("1,240 words");
    expect(sessionLine(320, 500)).toBe("This session: 320 of 500");
    expect(share(250, 500)).toBe(0.5);
    expect(share(900, 500)).toBe(1);
    expect(share(10, 0)).toBeNull();
    expect(clock(65)).toBe("1:05");
    expect(clock(3725)).toBe("1:02:05");
    expect(parseGoal(" 3,000 ")).toBe(3000);
    expect(parseGoal("lots")).toBeNull();
    expect(parseGoal("0")).toBeNull();
  });
});

describe("the manuscript navigator", () => {
  // blocks: 0 "Chapter One" (2 words), 1-2 prose, 3 "A Scene" (2), 4 prose, 5 "Chapter Two" (2), 6 prose
  const headings: Heading[] = [
    { blockIndex: 0, level: 1, text: "Chapter One" },
    { blockIndex: 3, level: 2, text: "A Scene" },
    { blockIndex: 5, level: 1, text: "Chapter Two" },
  ];
  const words = [2, 100, 50, 2, 30, 2, 400];

  it("counts each section's words, chapters including their scenes", () => {
    const counts = sectionCounts(headings, words);
    expect(counts.map((c) => [c.text, c.words, c.ownWords])).toEqual([
      ["Chapter One", 180, 150],
      ["A Scene", 30, 30],
      ["Chapter Two", 400, 400],
    ]);
    expect(countWords("It was — a  bright day.")).toBe(5);
    expect(wordsLabel(1)).toBe("1 word");
    expect(wordsLabel(12400)).toBe("12,400 words");
  });

  it("moves a whole section before another heading", () => {
    // chapter two, with its prose, before chapter one
    expect(moveSection(headings, 7, 2, 0)).toEqual([5, 6, 0, 1, 2, 3, 4]);
    // chapter one (with its scene) to the end
    expect(moveSection(headings, 7, 0, null)).toEqual([5, 6, 0, 1, 2, 3, 4]);
    // a chapter cannot move inside itself
    expect(moveSection(headings, 7, 0, 1)).toBeNull();
  });
});
