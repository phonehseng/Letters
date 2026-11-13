// Convert File…: names suggested beside the original, the first type
// chosen, and what is said about files converted elsewhere.

import { describe, expect, it } from "vitest";
import { elsewhereSentence, fileName, firstAvailable, savedSentence, suggestedPath, type ConvertTarget } from "./convertFile";

describe("convert file", () => {
  it("suggests the same name beside the original with the new extension", () => {
    expect(suggestedPath("C:\\School\\Essay.docx", "odt")).toBe("C:\\School\\Essay.odt");
    expect(suggestedPath("/Users/sam/Lab.v2.xlsx", "csv")).toBe("/Users/sam/Lab.v2.csv");
    // a dot in a folder name is not an extension
    expect(suggestedPath("/Users/sam/notes.d/README", "odt")).toBe("/Users/sam/notes.d/README.odt");
  });

  it("chooses the first type this computer can make", () => {
    const targets: ConvertTarget[] = [
      { format: "pdf", label: "PDF", extension: "pdf", available: false, note: "" },
      { format: "odt", label: "OpenDocument", extension: "odt", available: true, note: "" },
    ];
    expect(firstAvailable(targets)).toBe("odt");
    expect(firstAvailable([])).toBeNull();
  });

  it("sends pdfs and pictures to their own tools, and says what it converts", () => {
    expect(elsewhereSentence("pdf")).toContain("Export To…");
    expect(elsewhereSentence("picture")).toContain("New from Images…");
    expect(elsewhereSentence("other")).toContain("can’t convert");
    expect(elsewhereSentence("document")).toBeNull();
  });

  it("says where the new file went", () => {
    expect(fileName("C:\\School\\Essay.odt")).toBe("Essay.odt");
    expect(savedSentence({ path: "C:\\School\\Essay.odt", notes: [] })).toBe("Saved “Essay.odt” in School.");
  });
});
