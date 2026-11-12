// Form Data's plain-words pieces: the suggested file name and what an
// import did.

import { describe, expect, it } from "vitest";
import { dataFileBeside, importSummary } from "./formDataApi";

describe("form data", () => {
  it("suggests a data file beside the pdf", () => {
    expect(dataFileBeside("C:\\Forms\\Trip.pdf", "xfdf")).toBe("C:\\Forms\\Trip answers.xfdf");
  });

  it("says how many answers were filled and why others were left out", () => {
    expect(importSummary({ filled: ["a"], skipped: [] })).toBe("1 answer filled in.");
    // a csv with several rows says which one filled the form
    expect(importSummary({ filled: ["a"], skipped: [], note: "This file holds 3 rows of answers; the first was used." })).toBe("1 answer filled in. This file holds 3 rows of answers; the first was used.");
    expect(importSummary({ filled: ["a", "b"], skipped: [{ field: "total", reason: "it is calculated" }] })).toBe(
      "2 answers filled in. 1 left out: total (it is calculated)",
    );
  });
});
