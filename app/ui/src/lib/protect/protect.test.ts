import { describe, expect, it } from "vitest";
import type { PageText } from "../pageTools/textSelection";
import { asItems, contextAround, foundSentence, marksFromFound, queryFor } from "./findMarks";
import { canUsePassword, confirmHint, isPattern, passwordStrength } from "./passwordStrength";
import {
  addMarks,
  asRedactionAreas,
  keepMatchingPages,
  markIds,
  markLabel,
  markedSentence,
  moveMark,
  orderedMarks,
  padTextBox,
  pageSignature,
  removeMark,
  type NewMark,
} from "./pendingMarks";
import { ALL_ALLOWED, isRestricted, restrictionSentence } from "./permissions";
import type { ProtectionStatus, RedactionPreview } from "./protectApi";
import { applyEffect, applySheetText, hasProtection, lockLine, markTextEffect, removePasswordEffect } from "./protectEffects";
import { coversNothing, itemSummary, picturesNote, reviewPages, reviewTotals, reviewWarnings, totalsSentence, wholesaleNote } from "./review";
import { SCRUB_KINDS, scrubRows, startingChoice } from "./scrubItems";

const box = (x: number, y: number, width = 50, height = 12) => ({ x, y, width, height });
const mark = (page: number, x: number, y: number, label = ""): NewMark => ({ page, rect: box(x, y), source: "area", label });

describe("pending marks", () => {
  it("adds marks with ids and skips one already in the same place", () => {
    const ids = markIds();
    const first = addMarks([], [mark(0, 10, 10), mark(1, 10, 10)], ids);
    expect(first.marks.map((each) => each.id)).toEqual(["mark-1", "mark-2"]);
    const again = addMarks(first.marks, [mark(0, 10.2, 10.1), mark(0, 80, 10)], ids);
    expect(again.added).toHaveLength(1);
    expect(again.marks).toHaveLength(3);
  });

  it("ignores boxes without a size", () => {
    expect(addMarks([], [{ ...mark(0, 1, 1), rect: box(1, 1, 0, 5) }], markIds()).marks).toEqual([]);
  });

  it("removes and moves one mark only", () => {
    const { marks } = addMarks([], [mark(0, 10, 10), mark(0, 10, 40)], markIds());
    expect(removeMark(marks, "mark-1").map((each) => each.id)).toEqual(["mark-2"]);
    expect(moveMark(marks, "mark-2", box(5, 5))[1].rect).toEqual(box(5, 5));
    expect(moveMark(marks, "mark-2", box(5, 5))[0].rect).toEqual(box(10, 10));
  });

  it("asks the engine for boxes in reading order, grouped by page", () => {
    const { marks } = addMarks([], [mark(1, 10, 50), mark(0, 10, 90), mark(1, 10, 20), mark(0, 10, 30)], markIds());
    expect(orderedMarks(marks).map((each) => each.id)).toEqual(["mark-4", "mark-2", "mark-3", "mark-1"]);
    expect(asRedactionAreas(marks)).toEqual([
      { page: 0, rects: [box(10, 30), box(10, 90)] },
      { page: 1, rects: [box(10, 20), box(10, 50)] },
    ]);
  });

  it("says how many areas are marked and where", () => {
    const ids = markIds();
    expect(markedSentence([], 5)).toBe("No areas are marked.");
    expect(markedSentence(addMarks([], [mark(1, 0, 0)], ids).marks, 5)).toBe("1 area marked on page 2.");
    const three = addMarks([], [mark(1, 0, 0), mark(1, 0, 40), mark(3, 0, 0)], ids).marks;
    expect(markedSentence(three, 5)).toBe("3 areas marked on pages 2 and 4.");
  });

  it("drops the marks of a page that changed, and keeps the rest", () => {
    const { marks } = addMarks([], [mark(0, 0, 0), mark(1, 0, 0)], markIds());
    const before = new Map([
      [0, pageSignature({ width: 612, height: 792 }, "Name: Sam")],
      [1, pageSignature({ width: 612, height: 792 }, "Page two")],
    ]);
    const turned = new Map([
      [0, pageSignature({ width: 792, height: 612 }, "Name: Sam")],
      [1, pageSignature({ width: 612, height: 792 }, "Page two")],
    ]);
    expect(keepMatchingPages(marks, before, before).droppedPages).toEqual([]);
    const after = keepMatchingPages(marks, before, turned);
    expect(after.droppedPages).toEqual([0]);
    expect(after.kept.map((each) => each.page)).toEqual([1]);
    expect(pageSignature(undefined, "")).toBe("gone");
  });

  it("labels a mark by its text, or by how it was made", () => {
    const [text, area] = addMarks([], [{ ...mark(0, 0, 0, " sam@uni.edu "), source: "found" }, mark(0, 0, 50)], markIds()).marks;
    expect(markLabel(text)).toBe("sam@uni.edu");
    expect(markLabel(area)).toBe("Area you drew");
  });

  it("grows text boxes a little but keeps them on the page", () => {
    expect(padTextBox(box(0.5, 10, 20, 10), { width: 100, height: 100 })).toEqual({ x: 0, y: 9, width: 21.5, height: 12 });
    expect(padTextBox(box(90, 95, 10, 5), { width: 100, height: 100 })).toEqual({ x: 89, y: 94, width: 11, height: 6 });
  });
});

describe("the review", () => {
  const ids = markIds();
  const marks = addMarks([], [mark(1, 10, 50), mark(0, 10, 10), mark(1, 10, 10)], ids).marks;
  const preview: RedactionPreview = {
    areas: [
      { page: 0, rect: box(10, 10), texts: ["Sam Lee"], images: 0, paths: 0, annotations: [], formValues: [] },
      { page: 1, rect: box(10, 10), texts: [], images: 1, paths: 2, annotations: ["Note: call me"], formValues: ["123"] },
      { page: 1, rect: box(10, 50), texts: [], images: 0, paths: 0, annotations: [], formValues: [] },
    ],
    report: { pages: [], warnings: [] },
  };
  const dryRun = {
    pages: [{ page: 1, glyphsRemoved: 0, imagesRemoved: 0, imagesRewritten: 1, pathsRemoved: 2, pathsCut: 0, pathsClipped: 0, shadingsClipped: 0, annotationsRemoved: 1, formValuesRemoved: 1, removedWholesale: ["The page picture can’t be cut, so it was removed whole."] }],
    warnings: ["“Sam Lee” still appears in the document’s Title.", "This file has attached files, which redaction does not change."],
  };

  it("puts each preview area beside its mark, page by page", () => {
    const pages = reviewPages(marks, preview, dryRun);
    expect(pages.map((page) => page.page)).toEqual([0, 1]);
    expect(pages[0].items[0]).toMatchObject({ markId: "mark-2", text: ["Sam Lee"] });
    expect(pages[1].items.map((item) => item.markId)).toEqual(["mark-3", "mark-1"]);
    expect(pages[1].picturesBlanked).toBe(1);
    expect(wholesaleNote(pages[1])).toBe("On page 2: The page picture can’t be cut, so it was removed whole.");
    expect(wholesaleNote(pages[0])).toBeNull();
  });

  it("sums up and says when a mark covers nothing", () => {
    const pages = reviewPages(marks, preview, dryRun);
    expect(reviewTotals(pages)).toEqual({ lines: 1, pictures: 1, drawings: 2, comments: 1, formValues: 1 });
    expect(itemSummary(pages[1].items[0])).toBe("1 picture, 2 drawings, 1 comment, 1 form answer");
    expect(coversNothing(pages[1].items[1])).toBe(true);
    expect(itemSummary(pages[1].items[1])).toBe("Nothing is under this mark.");
  });

  it("says the totals and the pictures in plain words, leaving out what is zero", () => {
    const pages = reviewPages(marks, preview, dryRun);
    expect(totalsSentence(reviewTotals(pages))).toBe("1 line of text, 1 picture, 2 drawings, 1 comment and 1 form answer will be removed.");
    expect(totalsSentence({ lines: 3, pictures: 0, drawings: 0, comments: 0, formValues: 0 })).toBe("3 lines of text will be removed.");
    expect(totalsSentence({ lines: 0, pictures: 0, drawings: 0, comments: 0, formValues: 0 })).toMatch(/^The marks cover nothing/);
    expect(picturesNote(pages[1])).toBe("1 picture is partly blanked, so the rest of it stays.");
    expect(picturesNote({ ...pages[1], picturesRemoved: 2, picturesBlanked: 1 })).toBe(
      "2 pictures are removed whole; 1 is partly blanked, so the rest of it stays.",
    );
    expect(picturesNote(pages[0])).toBeNull();
  });

  it("offers Find and Mark for words that stay elsewhere, and always mentions fonts when text goes", () => {
    const warnings = reviewWarnings(dryRun, true);
    expect(warnings[0]).toEqual({ text: dryRun.warnings[0], phrase: "Sam Lee" });
    expect(warnings[1].phrase).toBeNull();
    expect(warnings[2].text).toMatch(/^Fonts stay in the file/);
    expect(reviewWarnings(null, false)).toEqual([]);
  });
});

describe("password strength", () => {
  it("says nothing for an empty field", () => {
    expect(passwordStrength("").level).toBe("empty");
  });

  it("calls common passwords and patterns weak", () => {
    expect(passwordStrength("Password").level).toBe("weak");
    expect(passwordStrength("abcdefgh").level).toBe("weak");
    expect(passwordStrength("99999999").level).toBe("weak");
    expect(isPattern("87654321")).toBe(true);
    expect(isPattern("8765x321")).toBe(false);
  });

  it("asks for at least eight characters", () => {
    expect(passwordStrength("k9#Lm").hint).toBe("Too short. Use at least 8 characters.");
  });

  it("calls long or varied passwords strong, and the rest fair", () => {
    expect(passwordStrength("tulipkettleorbit").level).toBe("strong");
    expect(passwordStrength("Tulip-42ab").level).toBe("strong");
    expect(passwordStrength("tulipket").level).toBe("fair");
  });

  it("checks the two fields match", () => {
    expect(confirmHint("a", "")).toBe("");
    expect(confirmHint("abc", "abd")).toBe("The passwords don’t match.");
    expect(confirmHint("abc", "abc")).toBe("The passwords match.");
    expect(canUsePassword("abc", "abc")).toBe(true);
    expect(canUsePassword("", "")).toBe(false);
  });
});

describe("permissions", () => {
  it("knows when anything is restricted", () => {
    expect(isRestricted(ALL_ALLOWED)).toBe(false);
    expect(isRestricted({ ...ALL_ALLOWED, printing: "lowQuality" })).toBe(true);
  });

  it("says what is restricted as a request, in one sentence", () => {
    expect(restrictionSentence(ALL_ALLOWED)).toBe("Nothing is restricted.");
    expect(restrictionSentence({ ...ALL_ALLOWED, printing: "none", copying: false })).toBe("Asks other apps not to allow printing and copying.");
    expect(restrictionSentence({ ...ALL_ALLOWED, printing: "lowQuality", editing: false, assembling: false })).toBe(
      "Asks other apps not to allow printing at full quality, changing the content and changing pages.",
    );
  });
});

describe("hidden information", () => {
  it("lists what was found, with comments and form answers unticked", () => {
    const rows = scrubRows({
      findings: [
        { item: "documentInfo", count: 2, details: ["Author: Jane Doe", "Title: Essay"] },
        { item: "attachments", count: 0, details: [] },
        { item: "comments", count: 3, details: [] },
        { item: "formValues", count: 1, details: [] },
      ],
    });
    expect(rows.map((row) => row.item)).toEqual(["documentInfo", "comments", "formValues"]);
    expect(startingChoice(rows)).toEqual(["documentInfo"]);
    expect(SCRUB_KINDS.comments.ticked).toBe(false);
  });
});

describe("find and mark", () => {
  const text: PageText = {
    chars: [..."Call sam@uni.edu today"].map((ch, index) => ({ ch, rect: box(index * 6, 10, 6, 10), line: 0 })),
    lines: [box(0, 10, 132, 10)],
  };

  it("builds the engine's query", () => {
    expect(queryFor({ pattern: "emailAddresses" })).toEqual({ kind: "emailAddresses" });
    expect(queryFor({ phrase: "Sam", matchCase: true })).toEqual({ kind: "phrase", text: "Sam", matchCase: true });
  });

  it("shows the words around a match", () => {
    expect(contextAround(text, [box(30, 10, 66, 10)], 5)).toEqual({ before: "Call ", after: " toda…" });
    expect(contextAround(text, [box(30, 10, 66, 10)], 30)).toEqual({ before: "Call ", after: " today" });
    expect(contextAround(undefined, [box(0, 0)])).toEqual({ before: "", after: "" });
    // the line before holds something else: it is left out
    const twoLines: PageText = {
      chars: [
        ...[..."ID 4417"].map((ch, index) => ({ ch, rect: box(index * 6, 0, 6, 10), line: 0 })),
        ...[..."Mail a@b.cd now"].map((ch, index) => ({ ch, rect: box(index * 6, 20, 6, 10), line: 1 })),
      ],
      lines: [box(0, 0, 42, 10), box(0, 20, 90, 10)],
    };
    expect(contextAround(twoLines, [box(30, 20, 36, 10)])).toEqual({ before: "Mail ", after: " now" });
  });

  it("turns the ticked matches into one mark per line", () => {
    const items = asItems([
      { page: 0, text: "sam@uni.edu", rects: [box(30, 10, 66, 10)] },
      { page: 2, text: "lee@uni.edu", rects: [box(0, 0), box(0, 20)] },
    ]);
    items[0].chosen = false;
    expect(marksFromFound(items)).toHaveLength(2);
    expect(marksFromFound(items)[0]).toMatchObject({ page: 2, source: "found", label: "lee@uni.edu" });
    expect(foundSentence(items, (pages) => `pages ${pages.map((page) => page + 1).join(" and ")}`, false)).toBe("2 matches on pages 1 and 3.");
    expect(foundSentence([], () => "", false)).toBe("No matches.");
  });
});

describe("the sentences", () => {
  const status = (changes: Partial<ProtectionStatus>): ProtectionStatus => ({
    locked: false,
    lock: null,
    unlockedWith: null,
    changed: false,
    openPasswordOnSave: false,
    restrictionsOnSave: null,
    ownerPasswordSet: false,
    undecided: false,
    cleanSave: false,
    removedContent: false,
    ...changes,
  });
  const lock = { method: "AES-256", revision: 6, needsOpenPassword: true, permissions: ALL_ALLOWED };

  it("never promises that marking removes anything", () => {
    expect(markTextEffect(false, null, 3)).toMatch(/Nothing is removed until you apply/);
    expect(markTextEffect(false, [1], 3)).toBe("Marks the selected text on page 2 for redaction. Nothing is removed until you apply.");
  });

  it("says applying is permanent once saved, and what it does not cover", () => {
    const marks = addMarks([], [mark(0, 0, 0), mark(0, 0, 40), mark(2, 0, 0)], markIds()).marks;
    expect(applyEffect(marks, 3)).toBe("Removes what is under 3 areas on pages 1 and 3 from your working copy. It becomes permanent when you save.");
    const sheet = applySheetText(marks, 3);
    expect(sheet.title).toBe("Apply 3 redactions?");
    expect(sheet.message).toMatch(/Once you save, it can’t be brought back/);
    expect(sheet.notCovered).toHaveLength(3);
  });

  it("says when a password takes effect", () => {
    expect(lockLine(null)).toBeNull();
    expect(lockLine(status({ locked: true, lock }))).toBe("Locked with AES-256. Read only until you unlock it for editing.");
    expect(lockLine(status({ openPasswordOnSave: true, changed: true }))).toBe("Locked with AES-256 when you save.");
    expect(lockLine(status({ restrictionsOnSave: { ...ALL_ALLOWED, copying: false } }))).toBe(
      "Asks other apps not to allow copying. Takes effect when you save.",
    );
    expect(lockLine(status({ changed: true, lock }))).toBe("Saved without a password from now on.");
  });

  it("offers Remove Password only when there is something to remove", () => {
    expect(hasProtection(status({}))).toBe(false);
    expect(removePasswordEffect(status({}))).toBe("This file has no password or restrictions to remove.");
    expect(hasProtection(status({ openPasswordOnSave: true }))).toBe(true);
  });
});
