// which slides the slide show plays, its steps and its clock.

import { describe, expect, it } from "vitest";
import { elapsed, playList, step } from "./showOrder";

describe("the slide show", () => {
  const slides = [
    { id: "a", hidden: false },
    { id: "b", hidden: true },
    { id: "c", hidden: false },
  ];

  it("skips hidden slides unless the show starts on one", () => {
    expect(playList(slides, "a")).toEqual(["a", "c"]);
    expect(playList(slides, "b")).toEqual(["a", "b", "c"]);
  });

  it("goes past the last slide to the end screen and not before the first", () => {
    expect(step(0, 2, 1)).toBe(1);
    expect(step(1, 2, 1)).toBeNull();
    expect(step(0, 2, -1)).toBe(0);
  });

  it("shows the time since it began", () => {
    expect(elapsed(65_000)).toBe("01:05");
    expect(elapsed(3_725_000)).toBe("1:02:05");
  });
});
