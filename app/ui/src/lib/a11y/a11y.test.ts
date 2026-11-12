// tests for the Accessibility tab's pure modules: findings, the fix
// wizard, the heading ladder, languages, contrast maths, and the audit of
// the app's own colour tokens in app.css.
import { describe, expect, it } from "vitest";
import type { Finding, FixKind, Report } from "./a11yApi";
import { contrastRatio, hex, over, parseColour, ratioText, requiredRatio } from "./contrast";
import { countsLine, findingKey, groupFindings, hasSpot, isAutomatic, solvedSince, whereText, wizardFindings } from "./findings";
import { languageName, looksLikeLanguageCode, startingLanguage } from "./languages";
import { ladderChanges, ladderFrom, moveItem, movedText, setLevel, skippedLevels } from "./ladder";
import { auditTokens, tokensFor } from "./tokenAudit";
import { advance, back, closingText, currentStep, isFinished, progressText, refresh, skip, startWizard } from "./wizard";

function finding(id: string, severity: Finding["severity"], fix: FixKind | null, page: number | null = 0, element: number | null = null): Finding {
  return {
    id,
    severity,
    title: id,
    explanation: "",
    whyItMatters: "",
    fixDescription: "",
    details: "",
    where: {
      page,
      rect: page === null ? null : { x: 10, y: 20, width: 30, height: 40 },
      structurePath: null,
      element: element === null ? null : { number: element, generation: 0 },
      annotation: null,
    },
    fix,
    pdfua: "",
    contrast: null,
  };
}

function report(findings: Finding[]): Report {
  const count = (severity: Finding["severity"]) => findings.filter((each) => each.severity === severity).length;
  return {
    pageCount: 3,
    findings,
    summary: { mustFix: count("mustFix"), shouldFix: count("shouldFix"), checkByHand: count("checkByHand"), total: findings.length, fixable: 0 },
    score: { percent: 50, passed: 5, failed: 5, notApplicable: 0, notChecked: 1, byHand: 2, statement: "" },
    rules: [],
    machineChecksPass: false,
  };
}

const untagged = report([
  finding("missing-language", "mustFix", { kind: "setLanguage", suggested: null }, null),
  finding("not-tagged", "mustFix", { kind: "addTags" }, null),
  finding("low-contrast", "shouldFix", { kind: "changeTextColour", runs: [], suggested: { r: 0, g: 0, b: 0 } }, 1),
  finding("image-only-page", "shouldFix", { kind: "recogniseText", pages: [2] }, 2),
  finding("check-reading-order-logical", "checkByHand", null, null),
]);

describe("findings", () => {
  it("groups them as Must Fix, Should Fix and Check by Hand", () => {
    const groups = groupFindings(untagged.findings);
    expect(groups.map((group) => `${group.title}: ${group.findings.length}`)).toEqual(["Must Fix: 2", "Should Fix: 2", "Check by Hand: 1"]);
    expect(groupFindings([])).toEqual([]);
  });

  it("says where each one is", () => {
    expect(whereText(untagged.findings[2])).toBe("Page 2");
    expect(whereText(untagged.findings[0])).toBe("The whole document");
    const figure = finding("missing-alt-text", "mustFix", null, 4);
    figure.where.structurePath = "Document › Figure 2";
    expect(whereText(figure)).toBe("Page 5, in Document › Figure 2");
    expect(hasSpot(untagged.findings[0])).toBe(false);
    expect(hasSpot(figure)).toBe(true);
  });

  it("recognises the same problem after a check, by rule and place", () => {
    expect(findingKey(finding("missing-alt-text", "mustFix", null, 0, 12))).toBe("missing-alt-text@12.0");
    expect(findingKey(untagged.findings[2])).toBe("low-contrast@1:10,20");
    expect(findingKey(untagged.findings[0])).toBe("missing-language@doc");
  });

  it("puts adding tags first and leaves out fixes that belong to other tools", () => {
    const steps = wizardFindings(untagged).map((each) => each.id);
    expect(steps).toEqual(["not-tagged", "missing-language", "low-contrast"]);
  });

  it("offers one heading fix for every skipped heading", () => {
    const two = report([
      finding("heading-level-skipped", "shouldFix", { kind: "fixHeadingLevels" }, 0, 5),
      finding("heading-level-skipped", "shouldFix", { kind: "fixHeadingLevels" }, 1, 9),
    ]);
    expect(wizardFindings(two)).toHaveLength(1);
  });

  it("knows which fixes need no answer", () => {
    expect(isAutomatic({ kind: "showTitle" })).toBe(true);
    expect(isAutomatic({ kind: "setTitle", suggested: "x" })).toBe(false);
  });

  it("counts and compares checks", () => {
    expect(countsLine(untagged)).toBe("2 must fix, 2 should fix, 1 to check by hand");
    expect(countsLine(report([]))).toBe("Nothing was found.");
    const after = report(untagged.findings.filter((each) => each.id !== "missing-language"));
    expect(solvedSince(untagged, after).map((each) => each.id)).toEqual(["missing-language"]);
  });
});

describe("the fix wizard", () => {
  it("walks the fixable findings with Skip and Back", () => {
    let state = startWizard(untagged);
    expect(progressText(state)).toBe("1 of 3");
    expect(currentStep(state)?.finding.id).toBe("not-tagged");
    state = skip(state);
    expect(progressText(state)).toBe("2 of 3");
    expect(state.steps[0].status).toBe("skipped");
    state = back(state);
    expect(currentStep(state)?.finding.id).toBe("not-tagged");
    expect(back(state).index).toBe(0);
  });

  it("marks a step fixed when the check no longer finds it, and goes on", () => {
    let state = skip(startWizard(untagged));
    // the language is fixed: the next check doesn't have it
    const after = report(untagged.findings.filter((each) => each.id !== "missing-language"));
    state = advance(refresh(state, after));
    expect(state.steps[1].status).toBe("fixed");
    expect(currentStep(state)?.finding.id).toBe("low-contrast");
    expect(progressText(state)).toBe("3 of 3");
  });

  it("adds problems that only show up after a fix, and reopens one that came back", () => {
    let state = startWizard(untagged);
    const tagged = report([
      ...untagged.findings.filter((each) => each.id !== "not-tagged"),
      finding("missing-alt-text", "mustFix", { kind: "setAltText", target: { kind: "element", element: { number: 40, generation: 0 } } }, 0, 40),
    ]);
    state = advance(refresh(state, tagged));
    expect(state.steps.map((step) => step.finding.id)).toEqual(["not-tagged", "missing-language", "low-contrast", "missing-alt-text"]);
    expect(state.steps[0].status).toBe("fixed");
    expect(progressText(state)).toBe("2 of 4");
    // undo brings the problem back
    state = refresh(state, untagged);
    expect(state.steps[0].status).toBe("open");
  });

  it("ends, and says what happened", () => {
    let state = startWizard(report([finding("missing-title", "mustFix", { kind: "setTitle", suggested: null }, null)]));
    state = advance(refresh(state, report([])));
    expect(isFinished(state)).toBe(true);
    expect(currentStep(state)).toBeNull();
    expect(closingText(state)).toBe("1 of 1 fixed.");
    expect(progressText(startWizard(report([])))).toBe("Nothing to fix");
    expect(closingText(skip(startWizard(untagged)))).toBe("0 of 3 fixed, 1 skipped. Skipped problems are still in the list.");
  });
});

describe("the heading ladder and reading order", () => {
  const proposal = [
    { element: { number: 3, generation: 0 }, text: "Rivers", from: 1, to: 1 },
    { element: { number: 4, generation: 0 }, text: "The Rhine", from: 3, to: 2 },
    { element: { number: 5, generation: 0 }, text: "Sources", from: 4, to: 3 },
  ];

  it("starts from the proposal and sends only the changes", () => {
    const rows = ladderFrom(proposal);
    expect(ladderChanges(rows)).toEqual([
      { element: { number: 4, generation: 0 }, level: 2 },
      { element: { number: 5, generation: 0 }, level: 3 },
    ]);
    expect(skippedLevels(rows)).toEqual([]);
  });

  it("warns about a skipped level the person makes", () => {
    const rows = setLevel(ladderFrom(proposal), 2, 4);
    expect(skippedLevels(rows)).toEqual([2]);
    expect(setLevel(rows, 0, 9)[0].to).toBe(6);
    expect(skippedLevels(setLevel(rows, 0, 2))).toEqual([0, 2]);
  });

  it("moves blocks in the reading order", () => {
    expect(moveItem(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
    expect(moveItem(["a", "b"], 0, 5)).toEqual(["b", "a"]);
    expect(moveItem(["a", "b"], 4, 0)).toEqual(["a", "b"]);
    expect(movedText(1, 4)).toBe("Moved to 2 of 4.");
  });
});

describe("languages", () => {
  it("starts at the suggestion, the computer's language, or English", () => {
    expect(startingLanguage("fr", undefined)).toBe("fr");
    expect(startingLanguage(null, "en-GB")).toBe("en-GB");
    expect(startingLanguage(null, "de-AT")).toBe("de");
    expect(startingLanguage(null, "xx")).toBe("en");
  });

  it("checks the shape of a typed code and names known ones", () => {
    expect(looksLikeLanguageCode("zh-Hans")).toBe(true);
    expect(looksLikeLanguageCode("english")).toBe(false);
    expect(languageName("pt-BR")).toBe("Portuguese (Brazil)");
    expect(languageName("tlh")).toBe("tlh");
  });
});

describe("contrast maths", () => {
  it("matches WCAG's examples", () => {
    const black = parseColour("#000")!;
    const white = parseColour("#ffffff")!;
    expect(contrastRatio(black, white)).toBeCloseTo(21, 5);
    // #777 on white is the classic near miss: 4.48
    expect(contrastRatio(parseColour("#777777")!, white)).toBeCloseTo(4.48, 2);
    expect(contrastRatio(parseColour("#767676")!, white)).toBeGreaterThanOrEqual(4.5);
  });

  it("lays see-through colours over their background first", () => {
    const half = parseColour("rgb(0 0 0 / 0.5)")!;
    expect(over(half, parseColour("#fff")!).r).toBeCloseTo(127.5);
    expect(parseColour("rgba(10, 20, 30, 0.4)")).toEqual({ r: 10, g: 20, b: 30, a: 0.4 });
    expect(parseColour("blue")).toBeNull();
  });

  it("says ratios and sizes the way people read them", () => {
    expect(ratioText(4.4999)).toBe("4.4 to 1");
    expect(requiredRatio(12, false)).toBe(4.5);
    expect(requiredRatio(14, true)).toBe(3);
    expect(requiredRatio(18, false)).toBe(3);
    expect(hex({ r: 255, g: 8, b: 0 })).toBe("#ff0800");
  });
});

// the tests run in node, so app.css is read from disk. vite's css handling
// would give an empty stylesheet here, and the ui has no node types, so
// the module is named in a way the type checker doesn't follow
const fileSystem = "node:fs";
const { readFileSync } = (await import(/* @vite-ignore */ fileSystem)) as { readFileSync: (path: string, encoding: "utf8") => string };
const appCss = readFileSync("src/app.css", "utf8");

describe("the app's own colours (app.css)", () => {
  it("reads each mode's tokens, later rules winning", () => {
    expect(tokensFor(appCss, "light")["--window"]).toBe("#f5f5f7");
    expect(tokensFor(appCss, "dark")["--window"]).toBe("#1e1e20");
    expect(tokensFor(appCss, "darkMoreContrast")["--glass"]).toBe("#2a2a2c");
  });

  it("gives text and controls enough contrast in every mode", () => {
    const failing = auditTokens(appCss)
      .filter((result) => !result.passes)
      .map((result) => `${result.mode}: ${result.name} ${result.ratio.toFixed(2)} < ${result.required}`);
    expect(failing).toEqual([]);
  });
});
