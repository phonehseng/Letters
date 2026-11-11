import { describe, expect, it } from "vitest";
import { nextIndex } from "./keyboard";

const menu = { orientation: "vertical", wrap: true } as const;
const tabs = { orientation: "horizontal", wrap: true } as const;
const tree = { orientation: "vertical", wrap: false } as const;

describe("nextIndex", () => {
  it("moves with the arrows that fit the orientation", () => {
    expect(nextIndex(0, "ArrowDown", 3, menu)).toBe(1);
    expect(nextIndex(1, "ArrowUp", 3, menu)).toBe(0);
    expect(nextIndex(0, "ArrowRight", 2, tabs)).toBe(1);
    expect(nextIndex(0, "ArrowDown", 2, tabs)).toBeNull();
  });

  it("goes round when wrapping and stops at the ends when not", () => {
    expect(nextIndex(2, "ArrowDown", 3, menu)).toBe(0);
    expect(nextIndex(0, "ArrowUp", 3, menu)).toBe(2);
    expect(nextIndex(2, "ArrowDown", 3, tree)).toBe(2);
    expect(nextIndex(0, "ArrowUp", 3, tree)).toBe(0);
  });

  it("jumps to the ends with Home and End", () => {
    expect(nextIndex(1, "Home", 5, tree)).toBe(0);
    expect(nextIndex(1, "End", 5, tree)).toBe(4);
  });

  it("ignores other keys and empty lists", () => {
    expect(nextIndex(0, "a", 3, menu)).toBeNull();
    expect(nextIndex(0, "ArrowDown", 0, menu)).toBeNull();
  });
});
