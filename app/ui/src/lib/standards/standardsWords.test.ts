// Prepare & Check's plain words.

import { describe, expect, it } from "vitest";
import type { Finding, PdfaReport } from "./standardsApi";
import { checkHeadline, convertedName, findingPlace, optimizeHeadline, readableSize } from "./standardsWords";

const finding = (page: number | null, object: string | null): Finding => ({
  rule: "fonts.notEmbedded",
  clause: "ISO 19005-2 6.2.11.4.1",
  message: "A font isn’t embedded.",
  page,
  object,
  fix: "needsFont",
});

describe("prepare and check words", () => {
  it("sizes read as bytes, KB or MB", () => {
    expect(readableSize(820)).toBe("820 bytes");
    expect(readableSize(348_160)).toBe("340 KB");
    expect(readableSize(1_258_291)).toBe("1.2 MB");
  });

  it("says where a finding is", () => {
    expect(findingPlace(finding(2, "12 0 R"))).toBe("Page 3, 12 0 R");
    expect(findingPlace(finding(0, null))).toBe("Page 1");
    expect(findingPlace(finding(null, "4 0 R"))).toBe("Object 4 0 R");
    expect(findingPlace(finding(null, null))).toBe("The whole file");
  });

  it("sums up a check and what converting can fix", () => {
    const report: PdfaReport = { level: "a2b", compliant: false, findings: [finding(0, null), finding(1, null)], automaticallyFixable: 1, note: "" };
    expect(checkHeadline(report)).toBe("2 problems against PDF/A-2b. Converting can fix 1 by itself.");
    expect(checkHeadline({ ...report, compliant: true, findings: [] })).toBe("Letters’ checker found nothing against PDF/A-2b.");
  });

  it("says how much smaller a copy is", () => {
    expect(optimizeHeadline({ bytesBefore: 2_000_000, bytesAfter: 500_000, measures: [], notDone: [] })).toBe(
      "Saved a copy of 488 KB, 75% smaller than 1.9 MB.",
    );
  });

  it("names the converted copy after its level, without a slash", () => {
    expect(convertedName("Thesis.pdf", "a2b")).toBe("Thesis (PDF-A-2b).pdf");
  });
});
