// what a signature check means, in plain words, and the guard that stands
// in front of the first change to a signed document.
//
// a digital signature covers the file's exact bytes. a change to a signed
// document is saved after the signed part (an incremental update, see
// shell-core's forms/signed.rs), so the signature still checks out but
// lists the change as made since. the person is asked first, and may keep
// the signed file as it is and work on a copy.

import type { SignatureReport } from "./formsApi";

export type SignatureStatus = "valid" | "changed" | "damaged";

/**
 * - valid: the bytes signed are unchanged and the signature is correct
 *   (another signature added after it counts as unchanged).
 * - changed: the file has changed since it was signed: bytes were added
 *   after the signed part, or the signed bytes themselves differ. a file
 *   saved again whole (as every edit but signing does) is this kind: the
 *   signed part no longer sits where the signature says.
 * - damaged: the signature can't be read, although nothing follows it.
 */
export function signatureStatus(report: SignatureReport): SignatureStatus {
  if (report.valid) {
    const onlySignedAfter = report.laterUpdates === 0 || report.laterUpdatesAreSignatures;
    return report.changedAfterSigning && !onlySignedAfter ? "changed" : "valid";
  }
  if (report.changedAfterSigning) return "changed";
  // the signature was read, so it is there; the bytes under it differ
  return report.certificate !== null && !report.digestMatches ? "changed" : "damaged";
}

export const STATUS_LABELS: Record<SignatureStatus, string> = {
  valid: "Valid",
  changed: "Changed after signing",
  damaged: "Damaged",
};

export function signerName(report: SignatureReport): string {
  return report.signer ?? report.name ?? "Unknown signer";
}

/** one sentence about a signature, for the list and screen readers. */
export function statusSentence(report: SignatureReport): string {
  switch (signatureStatus(report)) {
    case "valid":
      return "The document hasn’t changed since it was signed.";
    case "changed":
      return "The document has changed since it was signed.";
    default:
      return "This signature can’t be checked. The file may be damaged.";
  }
}

/** whether the signer made their own certificate, which nobody vouches for. */
export function isSelfSigned(report: SignatureReport): boolean {
  return report.trust === "selfSigned";
}

/** the quiet banner over a signed document. */
export function bannerText(reports: SignatureReport[]): string {
  if (reports.length === 0) return "";
  const statuses = reports.map(signatureStatus);
  if (reports.length === 1) {
    const who = signerName(reports[0]);
    if (statuses[0] === "valid") return `Signed by ${who}. It hasn’t changed since.`;
    if (statuses[0] === "changed") return `Signed by ${who}, but it has changed since.`;
    return `Signed by ${who}, but the signature can’t be checked.`;
  }
  const problems = statuses.filter((status) => status !== "valid").length;
  if (problems === 0) return `${reports.length} signatures. The document hasn’t changed since.`;
  return `${reports.length} signatures. ${problems === 1 ? "One shows" : `${problems} show`} changes or problems.`;
}

// ---- the guard before changing a signed document ----

export type GuardChoice = "saveCopy" | "editAnyway" | "cancel";

export interface GuardState {
  /** the saved file is signed (the shell protects it) */
  protected: boolean;
  /** the person already chose Edit Anyway or Save a Copy for this document */
  acknowledged: boolean;
}

export const SIGNED_EDIT_TITLE = "This document is signed.";
export const SIGNED_EDIT_QUESTION =
  "Your changes will be saved after the signature. It stays valid for what was signed, and lists what changed since. Keep the signed file as it is and save a copy instead?";

/** whether a change must ask first. undo and redo only move between versions, so they never do. */
export function mustAsk(state: GuardState, isHistory: boolean): boolean {
  return state.protected && !state.acknowledged && !isHistory;
}

/** what happens after the question: whether the change goes ahead, and the guard from then on. */
export function afterChoice(state: GuardState, choice: GuardChoice): { proceed: boolean; saveCopyFirst: boolean; state: GuardState } {
  if (choice === "cancel") return { proceed: false, saveCopyFirst: false, state };
  return { proceed: true, saveCopyFirst: choice === "saveCopy", state: { ...state, acknowledged: true } };
}
