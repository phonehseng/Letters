// tests of the Export dialog's logic: the format list from capabilities, checking options, the result.

import { describe, expect, it } from "vitest";
import {
  checkOptions,
  closenessRows,
  defaultOptions,
  durationHint,
  effectiveStyle,
  formatChoices,
  progressText,
  requestFor,
  resultLine,
  saveFilter,
  suggestedName,
  unavailableReason,
  type Capability,
  type ExportCapabilities,
} from "./exportChoices";

function capability(to: string, style: Capability["style"], available = true, how = "Letters"): Capability {
  return { from: "pdf", to, style, how, note: `Makes ${to}.`, available };
}

function found(overrides: Partial<Record<string, boolean>> = {}): ExportCapabilities {
  return {
    capabilities: [
      capability("png", null),
      capability("jpeg", null),
      capability("tiff", null),
      capability("text", "easyToEdit"),
      capability("text", "exactMatch"),
      capability("html", "exactMatch"),
      capability("html", "easyToEdit"),
      capability("xlsx", null),
      capability("pptx", "exactMatch"),
      capability("docx", "exactMatch", overrides.exactWord ?? true),
      capability("docx", "easyToEdit"),
      capability("odt", "easyToEdit", overrides.odt ?? true, overrides.odt === false ? "Not in this version" : "Letters"),
      { from: "docx", to: "pdf", style: null, how: "Microsoft Word", note: "Laid out by Word.", available: true },
    ],
    exactMatchExplained: "Exact match keeps every page looking just like the PDF.",
    easyToEditExplained: "Easy to edit rebuilds the text as headings and paragraphs.",
  };
}

describe("capabilities become the format list", () => {
  it("lists the seven formats in order, word first with both styles", () => {
    const choices = formatChoices(found());
    expect(choices.map((choice) => choice.id)).toEqual(["word", "openDocument", "excel", "powerPoint", "images", "webPage", "text"]);
    const word = choices[0];
    expect(word.styles?.map((style) => style.label)).toEqual(["Exact Match", "Easy to Edit"]);
    expect(word.styles?.[0].explanation).toBe("Exact match keeps every page looking just like the PDF.");
  });

  it("plain text calls its styles by what they do", () => {
    const text = formatChoices(found()).find((choice) => choice.id === "text");
    expect(text?.styles?.map((style) => style.label)).toEqual(["Keep Layout", "Reading Order"]);
  });

  it("only office to pdf entries are left out", () => {
    expect(formatChoices(found()).every((choice) => choice.available)).toBe(true);
  });

  it("something not built says Coming soon, with the engine's reason", () => {
    const odt = formatChoices(found({ odt: false })).find((choice) => choice.id === "openDocument");
    expect(odt?.available).toBe(false);
    expect(odt?.note).toBe("Coming soon. Makes odt.");
    expect(unavailableReason(undefined)).toBe("Coming soon.");
  });

  it("an unavailable style falls back to the other one", () => {
    const choices = formatChoices(found({ exactWord: false }));
    const word = choices[0];
    expect(word.styles?.[0].available).toBe(false);
    expect(effectiveStyle(word, "exactMatch")).toBe("easyToEdit");
    expect(requestFor(defaultOptions(), choices)).toEqual({ format: "word", style: "easyToEdit" });
  });
});

describe("options and checking", () => {
  const choices = formatChoices(found());

  it("starts as an exact match word document of every page", () => {
    const checked = checkOptions(defaultOptions(), 10, choices);
    expect(checked).toEqual({ ok: true, request: { format: "word", style: "exactMatch" }, pages: "all" });
  });

  it("settings can make easy to edit the starting style", () => {
    expect(defaultOptions("easyToEdit").styles.word).toBe("easyToEdit");
  });

  it("reads a typed page range into 0-based pages", () => {
    const options = { ...defaultOptions(), format: "excel" as const, pages: "range" as const, pagesText: "1-2, 5" };
    const checked = checkOptions(options, 10, choices);
    expect(checked.ok && checked.pages).toEqual({ pages: [0, 1, 4] });
  });

  it("says what is wrong with a range", () => {
    const options = { ...defaultOptions(), format: "excel" as const, pages: "range" as const, pagesText: "" };
    const checked = checkOptions(options, 10, choices);
    expect(checked.ok).toBe(false);
  });

  it("exact match word needs every page", () => {
    const options = { ...defaultOptions(), pages: "range" as const, pagesText: "2" };
    const checked = checkOptions(options, 10, choices);
    expect(checked).toEqual({ ok: false, error: "Exact Match makes the whole document. Choose All Pages, or Easy to Edit for some pages." });
  });

  it("checks picture settings", () => {
    const tooFine = { ...defaultOptions(), format: "images" as const, dpi: 5000 };
    expect(checkOptions(tooFine, 3, choices)).toEqual({ ok: false, error: "Choose a resolution between 10 and 2400 dpi." });
    const jpeg = { ...defaultOptions(), format: "images" as const, image: "jpeg" as const, quality: 70, color: "grayscale" as const };
    expect(requestFor(jpeg)).toEqual({ format: "images", image: { kind: "jpeg", quality: 70 }, dpi: 150, color: "grayscale" });
    const tiff = { ...defaultOptions(), format: "images" as const, image: "tiff" as const, multiPage: false };
    expect(requestFor(tiff)).toEqual({ format: "images", image: { kind: "tiff", compression: "lzw", multiPage: false }, dpi: 150, color: "color" });
  });

  it("a web page can be one file or keep its pictures beside it", () => {
    const page = { ...defaultOptions(), format: "webPage" as const, singleFile: false };
    expect(requestFor(page)).toEqual({ format: "webPage", style: "exactMatch", singleFile: false });
  });

  it("names the file after the pdf, with the format's extension", () => {
    expect(suggestedName("C:\\work\\Essay.pdf", defaultOptions())).toBe("C:\\work\\Essay.docx");
    expect(saveFilter({ ...defaultOptions(), format: "images", image: "tiff" }).extensions).toEqual(["tiff", "tif"]);
  });
});

describe("while and after it runs", () => {
  it("says how long exact match takes, and when it can't be checked", () => {
    expect(durationHint({ format: "word", style: "exactMatch" }, true)).toContain("Word itself measures");
    expect(durationHint({ format: "word", style: "exactMatch" }, false)).toContain("can’t be checked");
    expect(durationHint({ format: "powerPoint" }, true)).toBeNull();
  });

  it("progress in words", () => {
    expect(progressText(0.4, null)).toBe("Exporting… 40%");
    expect(progressText(0.4, 3)).toBe("Exporting page 3…");
  });

  it("the result line joins the summary and the check", () => {
    const outcome = { files: ["a.docx"], summary: "1 page with 3 lines of real text.", notes: [], verification: "Not checked against Word on this computer.", closeness: null, standIns: [] };
    expect(resultLine(outcome)).toBe("1 page with 3 lines of real text. Not checked against Word on this computer.");
  });

  it("how close is it, in plain rows", () => {
    const rows = closenessRows({ glyphs: 1234, maxPt: 0.09, meanPt: 0.031, p99Pt: 0.07, within0025: 0.5, within006: 0.9, within0125: 1, pass: true });
    expect(rows[0]).toEqual({ label: "Letters measured", value: "1,234" });
    expect(rows[1].value).toBe("0.090 pt");
    expect(rows.at(-1)?.value).toBe("Yes");
  });
});
