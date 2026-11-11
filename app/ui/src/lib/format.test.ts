import { describe, expect, it } from "vitest";
import { matchesSearch, plural, timeAgo } from "./format";

describe("timeAgo", () => {
  const now = 1_000_000;

  it("uses friendly words for recent times", () => {
    expect(timeAgo(now - 5, now)).toBe("just now");
    expect(timeAgo(now - 60, now)).toBe("1 minute ago");
    expect(timeAgo(now - 3 * 3600, now)).toBe("3 hours ago");
    expect(timeAgo(now - 30 * 3600, now)).toBe("yesterday");
    expect(timeAgo(now - 5 * 86400, now)).toBe("5 days ago");
  });

  it("does not go negative when the clock moved backwards", () => {
    expect(timeAgo(now + 500, now)).toBe("just now");
  });
});

describe("plural", () => {
  it("adds an s only when needed", () => {
    expect(plural(1, "page")).toBe("1 page");
    expect(plural(12, "page")).toBe("12 pages");
  });
});

describe("matchesSearch", () => {
  it("ignores case and word order", () => {
    expect(matchesSearch("Biology Lab Report 3.pdf", "report bio")).toBe(true);
    expect(matchesSearch("Biology Lab Report 3.pdf", "chemistry")).toBe(false);
  });

  it("matches everything when the box is empty", () => {
    expect(matchesSearch("anything.pdf", "   ")).toBe(true);
  });
});
