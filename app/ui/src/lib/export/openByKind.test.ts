// tests of which files are shown as pdfs made by Office or LibreOffice.

import { describe, expect, it } from "vitest";
import { isShownAsPdf, sheetShownAsPdfSentence, shownAsPdfSentence } from "./openByKind";

describe("files shown as pdfs", () => {
  it("spreadsheets and presentations, of both families", () => {
    for (const name of ["a.xlsx", "b.ODS", "c.pptx", "d.odp", "e.xls", "f.ppt"]) expect(isShownAsPdf(name)).toBe(true);
    for (const name of ["a.pdf", "b.docx", "c.rtf", "d.png"]) expect(isShownAsPdf(name)).toBe(false);
  });

  it("says who laid it out and how to keep it", () => {
    expect(shownAsPdfSentence("C:\\a\\Budget.xlsx", "Microsoft Excel")).toBe("Letters shows “Budget.xlsx” as a PDF laid out by Microsoft Excel. To keep it, choose Save As.");
  });
});

describe("spreadsheets the engine can't read", () => {
  it("say why and how to make them editable", () => {
    const sentence = sheetShownAsPdfSentence("C:\\a\\Old.xls", "Microsoft Excel", "This is an older Excel file (.xls).");
    expect(sentence).toBe("Letters couldn’t open “Old.xls” as a spreadsheet (This is an older Excel file (.xls)), so it shows a PDF laid out by Microsoft Excel. To edit it, save it as .xlsx in Microsoft Excel first.");
  });
});
