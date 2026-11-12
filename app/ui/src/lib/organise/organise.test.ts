// tests for choosing documents, the folder picker and organising's sentences.
import { describe, expect, it } from "vitest";
import { clickChoose, conflictQuestion, destinations, doneSentence, keepListed, NOTHING_CHOSEN, recycleQuestion, rememberDestination } from "./organise";

const list = ["a.pdf", "b.pdf", "c.pdf", "d.pdf"];

describe("choosing documents", () => {
  it("a plain click opens instead of choosing", () => {
    expect(clickChoose(NOTHING_CHOSEN, list, "b.pdf", { toggle: false, range: false })).toBeNull();
  });

  it("ctrl-click adds and takes away, shift-click chooses a run", () => {
    let chosen = clickChoose(NOTHING_CHOSEN, list, "b.pdf", { toggle: true, range: false })!;
    chosen = clickChoose(chosen, list, "d.pdf", { toggle: true, range: false })!;
    expect(chosen.paths).toEqual(["b.pdf", "d.pdf"]);
    chosen = clickChoose(chosen, list, "d.pdf", { toggle: true, range: false })!;
    expect(chosen.paths).toEqual(["b.pdf"]);
    chosen = clickChoose({ paths: ["b.pdf"], anchor: "b.pdf" }, list, "d.pdf", { toggle: false, range: true })!;
    expect(chosen.paths).toEqual(["b.pdf", "c.pdf", "d.pdf"]);
    expect(keepListed(chosen, ["b.pdf", "x.pdf"]).paths).toEqual(["b.pdf"]);
  });
});

describe("the folder picker", () => {
  it("lists pinned, library and recent places once, and filters as you type", () => {
    const found = destinations(["C:\\Biology 101"], ["C:\\Library", "c:\\biology 101"], ["D:\\Old"], "");
    expect(found.map((d) => d.kind)).toEqual(["pinned", "library", "recent"]);
    expect(destinations(["C:\\Biology 101"], ["C:\\Library"], [], "bio").map((d) => d.path)).toEqual(["C:\\Biology 101"]);
    expect(rememberDestination(["a", "b"], "B")).toEqual(["B", "a"]);
  });
});

describe("what organising says", () => {
  it("uses the finder's words", () => {
    expect(doneSentence("moved", 3, "C:\\Courses\\Biology 101")).toBe("Moved 3 documents to Biology 101.");
    expect(doneSentence("copied", 1, "C:\\X")).toBe("Copied 1 document to X.");
    expect(conflictQuestion("C:\\X\\Essay.pdf")).toContain("An item named “Essay.pdf” already exists in this location.");
    expect(recycleQuestion(["C:\\a\\Essay.pdf"], false)).toBe("Move “Essay.pdf” to the Recycle Bin? You can put it back from the Recycle Bin.");
    expect(recycleQuestion(["a", "b"], true)).toBe("Move 2 documents to the Trash? You can put them back from the Trash.");
  });
});
