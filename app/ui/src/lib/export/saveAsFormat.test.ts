// Save As asks for the format first: which one starts chosen, cancelling,
// and a second request while one is showing.

import { describe, expect, it } from "vitest";
import { chooseSaveFormat, saveAsFormat, type FormatOption } from "./saveAsFormat.svelte";

const FORMATS: FormatOption[] = [
  { id: "docx", label: "Word document", extension: "docx", available: true, note: "" },
  { id: "odt", label: "OpenDocument text", extension: "odt", available: true, note: "" },
  { id: "rtf", label: "Rich Text", extension: "rtf", available: false, note: "Needs Word or LibreOffice." },
];

describe("save as format", () => {
  it("starts on the document's own format and answers with the one chosen", async () => {
    const answer = chooseSaveFormat(FORMATS, "odt");
    expect(saveAsFormat.current?.chosen).toBe("odt");
    saveAsFormat.current?.answer(FORMATS[0]);
    expect(await answer).toBe(FORMATS[0]);
    expect(saveAsFormat.current).toBeNull();
  });

  it("starts on the first usable format when the own one can't be made here", async () => {
    const answer = chooseSaveFormat(FORMATS, "rtf");
    expect(saveAsFormat.current?.chosen).toBe("docx");
    saveAsFormat.current?.answer(null);
    expect(await answer).toBeNull();
  });

  it("cancels a request still showing when another one comes", async () => {
    const first = chooseSaveFormat(FORMATS, "docx");
    const second = chooseSaveFormat(FORMATS, "odt");
    expect(await first).toBeNull();
    saveAsFormat.current?.answer(FORMATS[1]);
    expect(await second).toBe(FORMATS[1]);
  });
});
