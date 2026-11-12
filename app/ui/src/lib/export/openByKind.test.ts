// tests of which files are shown as pdfs made by Office or LibreOffice.

import { describe, expect, it } from "vitest";
import { isShownAsPdf, shownAsPdfSentence } from "./openByKind";

describe("files shown as pdfs", () => {
  it("spreadsheets and presentations, of both families", () => {
    for (const name of ["a.xlsx", "b.ODS", "c.pptx", "d.odp", "e.xls", "f.ppt"]) expect(isShownAsPdf(name)).toBe(true);
    for (const name of ["a.pdf", "b.docx", "c.rtf", "d.png"]) expect(isShownAsPdf(name)).toBe(false);
  });

  it("says who laid it out and how to keep it", () => {
    expect(shownAsPdfSentence("C:\\a\\Budget.xlsx", "Microsoft Excel")).toBe("Letters shows “Budget.xlsx” as a PDF laid out by Microsoft Excel. To keep it, choose Save As.");
  });
});
