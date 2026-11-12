// a signature with changes saved after it reads as the engine judged
// those changes: allowed ones keep it valid.

import { describe, expect, it } from "vitest";
import type { SignatureReport } from "./formsApi";
import { bannerText, signatureStatus, statusSentence } from "./signatures";

const changedSince = {
  field: "Signature 1",
  signer: "Ada Lovelace",
  valid: true,
  digestMatches: true,
  changedAfterSigning: true,
  laterUpdates: 2,
  laterUpdatesAreSignatures: false,
  certificate: null,
} as unknown as SignatureReport;

describe("signatures with later changes", () => {
  it("changes the author allowed keep the signature valid", () => {
    expect(signatureStatus(changedSince, "asIntended")).toBe("allowed");
    expect(statusSentence(changedSince, "asIntended")).toBe("What was signed hasn’t changed; the changes since are ones the author allowed.");
    expect(bannerText([changedSince], ["asIntended"])).toBe("Signed by Ada Lovelace. The changes since are ones the author allowed.");
    expect(bannerText([changedSince, changedSince], ["asIntended", "unchanged"])).toBe("2 signatures. The changes since are ones the author allowed.");
  });

  it("other changes still show as changed, and without a verdict nothing is assumed", () => {
    expect(signatureStatus(changedSince, "notAllowed")).toBe("changed");
    expect(signatureStatus(changedSince, "noLimits")).toBe("changed");
    expect(signatureStatus(changedSince)).toBe("changed");
    expect(bannerText([changedSince, changedSince], ["asIntended", "notAllowed"])).toBe("2 signatures. One shows changes or problems.");
  });
});
