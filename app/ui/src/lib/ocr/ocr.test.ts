// tests for the Scan tab's pure parts: the run's state machine (with
// cancel), which pages are read, languages, the sentences, and putting
// pictures in order for New from Images.
import { describe, expect, it } from "vitest";
import { addPictures, isPicturePath, makeSentence, moveItem, picturePathsAmong } from "./imagesToPdf";
import { defaultLanguage, languageAdvice, languageNames, toggleLanguage } from "./languages";
import { suggestedPathFor } from "./newDocuments";
import type { JobStatus, OcrSummary } from "./ocrApi";
import { IDLE, isBusy, jobOf, nextView, type JobView } from "./ocrJob";
import {
  bannerSentence,
  cancelledSentence,
  confidenceSentence,
  editableEffect,
  editableSentence,
  lowPagesSentence,
  progressSentence,
  recognizeEffect,
  SCAN_MODES,
  summarySentence,
} from "./ocrSentences";
import { availableChoices, defaultChoice, pagesFor, pagesInWords } from "./pageRange";

const summary = (overrides: Partial<OcrSummary> = {}): OcrSummary => ({
  pages: [0, 1, 2, 3],
  pagesWithText: 3,
  blankPages: [3],
  meanConfidence: 91.4,
  lowConfidencePages: [],
  wordCount: 120,
  ...overrides,
});

const status = (state: JobStatus["state"], done: number, overrides: Partial<JobStatus> = {}): JobStatus => ({
  id: 5,
  document: 1,
  state,
  progress: { done, total: 4, current: done < 4 ? done : null },
  error: null,
  summary: null,
  ...overrides,
});

describe("the job state machine", () => {
  const running = (): JobView => nextView(nextView(IDLE, { type: "start" }), { type: "started", job: 5, total: 4 });

  it("runs, reports each page, then adds the text", () => {
    let view = running();
    expect(view).toEqual({ phase: "running", job: 5, done: 0, total: 4, current: null });
    view = nextView(view, { type: "status", status: status("running", 2) });
    expect(view).toMatchObject({ phase: "running", done: 2 });
    expect(isBusy(view)).toBe(true);
    view = nextView(view, { type: "status", status: status("finished", 4) });
    expect(view).toEqual({ phase: "applying", job: 5 });
    view = nextView(view, { type: "applied", summary: summary() });
    expect(view.phase).toBe("done");
    expect(isBusy(view)).toBe(false);
    expect(nextView(view, { type: "reset" })).toBe(IDLE);
  });

  it("cancels, and a cancel stays asked for", () => {
    let view = nextView(running(), { type: "cancel" });
    expect(view).toMatchObject({ phase: "cancelling", job: 5 });
    // an older answer that still says running doesn't undo the cancel
    view = nextView(view, { type: "status", status: status("running", 1) });
    expect(view.phase).toBe("cancelling");
    view = nextView(view, { type: "status", status: status("cancelled", 1) });
    expect(view).toEqual({ phase: "cancelled", done: 1, total: 4 });
    expect(jobOf(view)).toBeNull();
  });

  it("drops the pages of a run that finished just after Cancel", () => {
    const view = nextView(nextView(running(), { type: "cancel" }), { type: "status", status: status("finished", 4) });
    expect(view).toEqual({ phase: "cancelled", done: 4, total: 4 });
  });

  it("shows a failure, and ignores answers about another run", () => {
    expect(nextView(running(), { type: "status", status: status("failed", 2, { error: "Page 3 couldn’t be read." }) })).toEqual({
      phase: "failed",
      message: "Page 3 couldn’t be read.",
    });
    const other = nextView(running(), { type: "status", status: status("finished", 4, { id: 9 }) });
    expect(other.phase).toBe("running");
  });

  it("doesn't start twice or reset while busy", () => {
    const view = running();
    expect(nextView(view, { type: "start" })).toBe(view);
    expect(nextView(view, { type: "reset" })).toBe(view);
    expect(nextView(IDLE, { type: "cancel" })).toBe(IDLE);
  });
});

describe("which pages are read", () => {
  const facts = { current: 2, selection: [5, 1, 1], pageCount: 8, scanned: [0, 1, 2, 3] };

  it("turns a choice into sorted pages", () => {
    expect(pagesFor("current", facts)).toEqual([2]);
    expect(pagesFor("selection", facts)).toEqual([1, 5]);
    expect(pagesFor("selection", { ...facts, selection: [] })).toEqual([2]);
    expect(pagesFor("all", facts)).toHaveLength(8);
    expect(pagesFor("scanned", facts)).toEqual([0, 1, 2, 3]);
  });

  it("offers only the choices that make sense, and starts with the scans", () => {
    expect(availableChoices(facts).map((choice) => choice.value)).toEqual(["current", "selection", "all", "scanned"]);
    expect(availableChoices({ ...facts, selection: [], scanned: [] }).map((choice) => choice.value)).toEqual(["current", "all"]);
    expect(defaultChoice(facts)).toBe("scanned");
    expect(defaultChoice({ ...facts, scanned: [0, 1, 2, 3, 4, 5, 6, 7] })).toBe("all");
    expect(defaultChoice({ ...facts, pageCount: 1, scanned: [0] })).toBe("current");
  });

  it("says pages in words", () => {
    expect(pagesInWords([2], 8)).toBe("page 3");
    expect(pagesInWords([0, 1, 2, 3, 6], 8)).toBe("pages 1–4 and 7");
    expect(pagesInWords([0, 2, 4], 8)).toBe("pages 1, 3 and 5");
    expect(pagesInWords([0, 1, 2], 3)).toBe("all 3 pages");
  });
});

describe("languages", () => {
  const installed = [
    { code: "deu", name: "German" },
    { code: "eng", name: "English" },
    { code: "fra", name: "French" },
    { code: "spa", name: "Spanish" },
  ];

  it("starts with the computer's language when it is there", () => {
    expect(defaultLanguage(installed, "fr-CA")).toBe("fra");
    expect(defaultLanguage(installed, "ja-JP")).toBe("eng");
    expect(defaultLanguage([{ code: "spa", name: "Spanish" }], "ja")).toBe("spa");
    expect(defaultLanguage([], "en")).toBeNull();
  });

  it("starts with the language chosen in settings when it is installed", () => {
    expect(defaultLanguage(installed, "fr-CA", "eng")).toBe("eng");
    expect(defaultLanguage(installed, "en-US", "fra")).toBe("fra");
    // a language no longer installed falls back to the computer's
    expect(defaultLanguage(installed, "fr-CA", "jpn")).toBe("fra");
    expect(defaultLanguage(installed, "fr-CA", "auto")).toBe("fra");
  });

  it("allows two at most, and says why", () => {
    let chosen = toggleLanguage([], "eng");
    chosen = toggleLanguage(chosen, "fra");
    expect(chosen).toEqual(["eng", "fra"]);
    expect(toggleLanguage(chosen, "deu")).toBe(chosen);
    expect(toggleLanguage(chosen, "eng")).toEqual(["fra"]);
    expect(languageAdvice(chosen)).toContain("slower");
    expect(languageAdvice([])).toBe("Choose the language the document is written in.");
    expect(languageNames(chosen, installed)).toBe("English and French");
  });
});

describe("the sentences", () => {
  it("summarises a run", () => {
    expect(summarySentence(summary())).toBe("Text found on 3 of 4 pages; 1 page looks blank.");
    expect(summarySentence(summary({ pagesWithText: 4, blankPages: [] }))).toBe("Text found on all 4 pages.");
    expect(summarySentence(summary({ pages: [6], pagesWithText: 1, blankPages: [] }))).toBe("Text found on page 7.");
    expect(summarySentence(summary({ pages: [0, 1, 2], pagesWithText: 1, blankPages: [1, 2] }))).toBe("Text found on 1 of 3 pages; 2 pages look blank.");
    expect(summarySentence(summary({ pagesWithText: 0, blankPages: [0, 1, 2, 3], wordCount: 0 }))).toContain("No text was found");
    expect(confidenceSentence(summary())).toBe("On average Letters was 91% sure of each word.");
    expect(lowPagesSentence(summary({ lowConfidencePages: [{ page: 2, confidence: 55 }, { page: 6, confidence: 61 }] }), 8)).toBe(
      "Check pages 3 and 7: some words there may be wrong. Show Recognized Text marks the doubtful ones.",
    );
    expect(lowPagesSentence(summary(), 8)).toBe("");
  });

  it("says how far a run got and what a cancel did", () => {
    expect(progressSentence({ phase: "running", job: 1, done: 2, total: 14, current: 5 })).toBe("Reading page 3 of 14…");
    expect(progressSentence({ phase: "cancelling", job: 1, done: 2, total: 14 })).toBe("Stopping…");
    expect(cancelledSentence({ phase: "cancelled", done: 3, total: 14 })).toBe("Stopped after 3 pages of 14. Nothing was added, so the document is as it was.");
    expect(cancelledSentence({ phase: "cancelled", done: 0, total: 14 })).toBe("Stopped before any page was read. Nothing was added.");
  });

  it("says what Recognize Text will do and what the banner is about", () => {
    expect(recognizeEffect([0, 1], 2, "English")).toMatch(/^Reads all 2 pages in English and adds an invisible text layer/);
    expect(recognizeEffect([], 2, "English")).toBe("Choose the pages to read.");
    expect(bannerSentence(3, 3)).toBe("This document is scanned, so its words can’t be searched or copied yet.");
    expect(bannerSentence(2, 10)).toBe("2 pages of this document are scans, so their words can’t be searched or copied yet.");
  });
});

describe("new from images", () => {
  it("knows pictures by their names", () => {
    expect(isPicturePath("C:\\Photos\\IMG_1.JPG")).toBe(true);
    expect(isPicturePath("scan.tiff")).toBe(true);
    expect(isPicturePath("essay.pdf")).toBe(false);
    expect(picturePathsAmong(["a.png", "b.pdf", "c.jpeg"])).toEqual(["a.png", "c.jpeg"]);
  });

  it("puts pictures in order without repeats", () => {
    expect(moveItem(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
    expect(moveItem(["a", "b"], 0, 5)).toEqual(["a", "b"]);
    expect(addPictures(["a.png"], ["a.png", "b.jpg", "notes.txt"])).toEqual(["a.png", "b.jpg"]);
  });

  it("says what will be made, and suggests a name beside the pictures", () => {
    expect(makeSentence(3, true, "English")).toBe(
      "Makes a PDF with 3 pages, in this order, reads their text in English so it can be searched, and opens it here. Nothing is saved until you choose Save As.",
    );
    expect(makeSentence(0, false, "English")).toBe("Choose or drop PNG, JPEG or TIFF pictures first.");
    expect(suggestedPathFor("C:\\Photos\\IMG_0012.jpg")).toBe("C:\\Photos\\IMG_0012.pdf");
  });
});

describe("make editable text", () => {
  it("says what becomes of the scan in one sentence for each way", () => {
    expect(SCAN_MODES.map((mode) => mode.label)).toEqual(["Text Over the Scan", "Text Only"]);
    expect(SCAN_MODES.every((mode) => mode.sentence.length > 0)).toBe(true);
  });

  it("says pages not read yet are read first", () => {
    expect(editableEffect([0], 3, 1)).toMatch(/^Reads page 1 first, then writes/);
    expect(editableEffect([0], 3, 0)).toMatch(/^Writes the words/);
    expect(editableEffect([], 3, 0)).toBe("Choose the pages to make editable.");
  });

  it("says how much became text and what to check", () => {
    expect(editableSentence({ pages: 1, lines: 14, doubtfulLines: 0, figures: 0 })).toBe("14 lines on 1 page are now editable text.");
    expect(editableSentence({ pages: 2, lines: 1, doubtfulLines: 1, figures: 2 })).toBe(
      "1 line on 2 pages is now editable text. 2 pictures from the scan were kept. Check 1 line against the scan: the reading was unsure.",
    );
  });
});
