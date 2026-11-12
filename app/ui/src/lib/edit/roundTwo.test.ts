// tests for the Edit tab's round-two pure parts: which typing re-wraps a
// paragraph, the quiet line after a change, the overflow choices, the
// question before changing a signed file, selecting and reshaping shapes,
// and the words of page setup. the page objects are built by hand in the
// shape core/edit answers.
import { describe, expect, it } from "vitest";
import { consentQuestion, extensionSentence, listed, overflowChoices, overflowTitle, summarySentence } from "./changeSentences";
import { ConsentGate } from "./consent.svelte";
import type { ChangesSummary, EditCheck, OverflowReport } from "./contentApi";
import type { FontInfo, PageObjects, PathObject, TextBlock, TextLine } from "./editApi";
import { estimatedWidth, flowingText, planFlow } from "./flowPlan";
import { inches, paperNamed, spaceSentence } from "./pageSetupWords";
import { movedPoints, selectionFor, shapesOf, targetRect, unionRect } from "./shapeGeometry";

const font: FontInfo = {
  id: "font:7-0",
  name: "ArialMT",
  family: "sansSerif",
  bold: false,
  italic: false,
  embedded: true,
  subset: true,
  kind: "composite",
  editableChars: "",
};

/** a line whose letters are 6 points wide. */
function line(id: string, text: string, y: number): TextLine {
  return {
    id,
    rect: { x: 72, y, width: text.length * 6, height: 12 },
    text,
    baseline: [
      { x: 72, y: y + 10 },
      { x: 72 + text.length * 6, y: y + 10 },
    ],
    runs: [{ id: `${id}-run`, text, font, size: 11, color: { r: 0, g: 0, b: 0 }, charBoxes: [], invisible: false }],
  };
}

/** a paragraph 60 letters (360 points) wide whose last line is short. */
function paragraph(): TextBlock {
  const first = "Thank you for your order of last week. We have reviewed it.";
  const last = "Please keep this letter.";
  return {
    id: "block-1",
    rect: { x: 72, y: 100, width: 360, height: 27 },
    text: `${first}\n${last}`,
    lines: [line("line-1", first, 100), line("line-2", last, 115)],
    rotation: 0,
    alignment: "justified",
  };
}

const NOTHING: ChangesSummary = {
  pagesChanged: [0],
  textLinesMoved: 0,
  imagesMoved: 0,
  pathsMoved: 0,
  annotationsMoved: 0,
  pathsReshaped: 0,
  fontsExtended: [],
  tightening: null,
  notes: [],
};

describe("which typing re-wraps the paragraph", () => {
  it("keeps a small change inside a line as the smallest edit", () => {
    const block = paragraph();
    const typed = block.text.replace("this letter", "this note");
    expect(planFlow(block, typed).kind).toBe("replaceLine");
    expect(planFlow(block, block.text.replace("letter.", "letter!")).kind).toBe("replaceLine");
  });

  it("re-wraps once a line would run past the paragraph's edge", () => {
    const block = paragraph();
    const typed = `${block.text} From 1 July, the café opens at 7 am — ask Quentin for the €5 voucher.`;
    const plan = planFlow(block, typed);
    expect(plan.kind).toBe("reflow");
    if (plan.kind === "reflow") expect(plan.text).toBe(flowingText(typed));
  });

  it("re-wraps when several lines changed, and deletes stay small", () => {
    const block = paragraph();
    expect(planFlow(block, "Thanks.\nKeep it.").kind).toBe("reflow");
    expect(planFlow(block, block.text.replace(" of last week", "")).kind).toBe("delete");
    expect(planFlow(block, "").kind).toBe("replaceBlock");
  });

  it("estimates a line's width from its own letters", () => {
    expect(estimatedWidth(line("a", "abcd", 0), 10)).toBe(60);
    expect(flowingText("one two\n  three \n\nfour")).toBe("one two three four");
  });
});

describe("the quiet line after a change", () => {
  it("says what moved and which way", () => {
    expect(summarySentence({ ...NOTHING, textLinesMoved: 3, imagesMoved: 1 }, 14.5)).toBe("3 lines and 1 picture moved down by 14.5 pt.");
    expect(summarySentence({ ...NOTHING, pathsMoved: 2 })).toBe("2 shapes moved.");
    expect(summarySentence(NOTHING)).toBe("");
  });

  it("names stretched rules, tightening, other pages and the engine's notes", () => {
    const summary: ChangesSummary = {
      ...NOTHING,
      pagesChanged: [0, 1],
      pathsReshaped: 3,
      tightening: { letterSpacingEm: -0.01, wordSpacingScale: 0.9, leadingScale: 1 },
      notes: ["Matched the style of “Thank you…”."],
    };
    expect(summarySentence(summary)).toBe(
      "3 rules stretched with the row. The paragraph was set a little tighter (word spaces 90% and letters -0.010 em closer). Pages 1 and 2 changed. Matched the style of “Thank you…”.",
    );
    expect(listed(["a", "b", "c"])).toBe("a, b and c");
  });

  it("uses the engine's sentence for letters added to a font", () => {
    const extension = { font: "ArialMT", characters: "é", glyphSource: "Arial on this computer", matchMethod: "same font", similarity: 100, sentence: "" };
    expect(extensionSentence(extension)).toBe("Letters added to ArialMT came from Arial on this computer.");
    expect(extensionSentence({ ...extension, sentence: "New letters: “é” from Arial on this computer, the same font." })).toContain("the same font");
  });
});

describe("the overflow choices", () => {
  const report: OverflowReport = {
    amount: 14.25,
    bottomLimit: 700,
    limitReason: "footer",
    choices: [
      { choice: "allowIntoMargin", possible: true, detail: "Let the text run 14.2 points into the bottom margin (allowed: 36.0)." },
      { choice: "tighten", possible: false, detail: "Even the tightest setting that keeps the look wouldn’t make it fit." },
      { choice: "carryToNextPage", possible: true, detail: "Move the lines that don’t fit to the top of the same column on the next page." },
      { choice: "leave", possible: true, detail: "Make the change and let the column run past its bottom; fix it by hand." },
    ],
  };

  it("says how far past what, and lists each way out with its sentence", () => {
    expect(overflowTitle(report)).toBe("The new text runs 14.3 pt past the footer.");
    const choices = overflowChoices(report);
    expect(choices.map((choice) => choice.label)).toEqual(["Run into the Margin", "Tighten", "Carry to Next Page", "Leave It"]);
    expect(choices[1]).toMatchObject({ possible: false, sentence: report.choices[1].detail });
  });
});

describe("the question before changing a signed file", () => {
  const signed: EditCheck = {
    locked: false,
    signatures: [{ field: "Signature1", signer: "Jane Doe", signedAt: null }],
    message: "This PDF is digitally signed by Jane Doe. Changing its content will make the signature invalid. Confirm to edit it anyway.",
  };

  it("asks once, naming the signer, and remembers the answer", async () => {
    let checks = 0;
    const gate = new ConsentGate(() => 1, async () => (checks++, signed));
    const asked = gate.ensure();
    await Promise.resolve();
    await Promise.resolve();
    expect(gate.asking?.message).toContain("Jane Doe");
    gate.asking!.answer(true);
    expect(await asked).toBe(true);
    expect(gate.consent).toEqual({ invalidateSignatures: true });
    expect(await gate.ensure()).toBe(true);
    expect(checks).toBe(1);
  });

  it("changes nothing when the person cancels, and never asks for an unsigned file", async () => {
    const gate = new ConsentGate(() => 1, async () => signed);
    let ran = false;
    const guarded = gate.guarded(async () => (ran = true));
    await Promise.resolve();
    await Promise.resolve();
    gate.asking!.answer(false);
    expect(await guarded).toBe(false);
    expect(ran).toBe(false);
    const unsigned = new ConsentGate(() => 1, async () => ({ locked: false, signatures: [], message: null }));
    expect(await unsigned.ensure()).toBe(true);
    expect(unsigned.asking).toBeNull();
    expect(consentQuestion(null, "Sam")).toContain("signed by Sam");
  });
});

describe("selecting and reshaping shapes", () => {
  const path = (id: string, x: number, width = 10, height = 10): PathObject => ({
    id,
    rect: { x, y: 50, width, height },
    shape: "rectangle",
    fill: null,
    stroke: null,
    fillOpacity: 1,
    strokeOpacity: 1,
    clipped: false,
    pointEditable: true,
    points: [],
  });
  const objects: PageObjects = {
    page: 0,
    width: 612,
    height: 792,
    textBlocks: [],
    images: [],
    paths: [path("a", 10), path("b", 30), path("c", 100)],
    pathGroups: [{ id: "logo", pathIds: ["a", "b"], rect: { x: 10, y: 50, width: 30, height: 10 }, reason: "cluster" }],
  };

  it("selects a logo as one, and a lone shape alone", () => {
    expect(selectionFor(objects, "b")).toEqual(["a", "b"]);
    expect(selectionFor(objects, "c")).toEqual(["c"]);
    expect(shapesOf(objects, ["c", "gone"]).map((shape) => shape.id)).toEqual(["c"]);
    expect(unionRect(shapesOf(objects, ["a", "b"]).map((shape) => shape.rect))).toEqual({ x: 10, y: 50, width: 30, height: 10 });
  });

  it("gives a hairline a target wide enough to click", () => {
    expect(targetRect({ x: 72, y: 100, width: 300, height: 0.5 })).toEqual({ x: 72, y: 97.25, width: 300, height: 6 });
  });

  it("keeps a rectangle a rectangle when one corner is dragged", () => {
    const corners = [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 0, y: 10 },
      { x: 10, y: 10 },
    ];
    expect(movedPoints("rectangle", corners, 3, { x: 20, y: 15 })).toEqual([
      { x: 0, y: 0 },
      { x: 20, y: 0 },
      { x: 0, y: 15 },
      { x: 20, y: 15 },
    ]);
    expect(movedPoints("line", corners.slice(0, 2), 1, { x: 5, y: 5 })).toEqual([
      { x: 0, y: 0 },
      { x: 5, y: 5 },
    ]);
  });
});

describe("the words of page setup", () => {
  it("names a paper size either way up, and says space in inches", () => {
    expect(paperNamed(612, 792)?.id).toBe("letter");
    expect(paperNamed(842, 595)?.id).toBe("a4");
    expect(paperNamed(500, 500)).toBeNull();
    expect(inches(36)).toBe("0.5 in");
    expect(spaceSentence("bottom", 72)).toBe("Adds 72 pt (1 in) of white space at the bottom. The content stays where it is.");
    expect(spaceSentence("left", -18)).toContain("Takes away 18 pt");
  });
});
