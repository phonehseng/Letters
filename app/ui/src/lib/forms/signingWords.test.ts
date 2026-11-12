// the plain words of signing extras and Verify.

import { describe, expect, it } from "vitest";
import type { SignatureDetails } from "./signingApi";
import { auditEventText, choicesProblem, hostOf, lockFor, noChoices, signChoicesEffect, timeLine } from "./signingWords";

describe("signing options", () => {
  it("locks nothing, every field, or the fields ticked", () => {
    expect(lockFor("none", ["a"])).toBeNull();
    expect(lockFor("all", [])).toEqual({ action: "all" });
    expect(lockFor("chosen", ["a", "b"])).toEqual({ action: "include", fields: ["a", "b"] });
    expect(lockFor("chosen", [])).toBeNull();
  });

  it("a trusted time needs a server chosen in Settings first", () => {
    const choices = { ...noChoices(), trustedTime: true };
    expect(choicesProblem(choices, "", "none")).toBe("Choose a time stamp server under Settings, Signing, or untick Add a Trusted Time.");
    expect(choicesProblem(choices, "http://127.0.0.1:3180/tsr", "none")).toBeNull();
    expect(choicesProblem(noChoices(), "", "chosen")).toBe("Tick the fields to lock, or choose Lock None.");
  });

  it("says everything signing will do, naming the server it asks", () => {
    const choices = { ...noChoices(), trustedTime: true, certify: "formFillingAndSigning" as const, lock: { action: "all" as const } };
    expect(signChoicesEffect(false, choices, "https://tsa.example.org/tsr")).toBe(
      "Signs the saved file, certifies it (filling in and signing allowed), locks every field, asks tsa.example.org for a trusted time and records the signature in the audit trail, then saves it again at once. Later changes are saved after the signature, which lists them.",
    );
    expect(signChoicesEffect(true, { ...noChoices(), auditTrail: false }, "")).toMatch(/^Saves your changes, then signs the saved file, then saves/);
  });

  it("names a server by its host", () => {
    expect(hostOf("http://127.0.0.1:3180/tsr")).toBe("127.0.0.1:3180");
    expect(hostOf("not an address")).toBe("not an address");
  });
});

describe("verify and the audit trail", () => {
  it("says where a signature's time comes from", () => {
    const clock: SignatureDetails = { field: "s", kind: "approval", timestamp: null, timeSource: "signerClock", timeNote: "" };
    expect(timeLine(clock, "2026-11-12 09:00")).toBe("Time from the signer’s computer: 2026-11-12 09:00.");
    const stamped: SignatureDetails = {
      ...clock,
      timeSource: "timestampAuthority",
      timestamp: { time: "2026-11-12 09:00:05 UTC", authority: "CN=Test TSA", imprintMatches: true, signatureCorrect: true, certificateForTimestamping: true, problems: [] },
    };
    expect(timeLine(stamped, null)).toBe("Time from a time stamp server (CN=Test TSA): 2026-11-12 09:00:05 UTC.");
  });

  it("describes each audit entry in a few words", () => {
    expect(auditEventText({ type: "prepared", note: null })).toBe("Prepared for filling in");
    expect(auditEventText({ type: "signed", field: "s", signer: "Ada", signingTime: "t", timeSource: "the signer's computer clock" })).toBe(
      "Signed by Ada (the signer's computer clock)",
    );
  });
});
