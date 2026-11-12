// the plain words of signing extras and Verify: the three certification
// levels, field locks, the verdict on what changed after a signature,
// the kinds of change, and the "what will happen" line under Sign. the
// engine's own sentences (the time stamp disclosure, the verdict sentence,
// the audit trail note) are shown as they are; these only label them.

import type { AuditEvent, CertificationLevel, ChangeKind, ChangeVerdict, FieldLock, SignatureDetails, SigningChoices } from "./signingApi";

/** the three permission levels in plain words, most strict first. */
export const CERTIFY_LABELS: Record<CertificationLevel, string> = {
  noChanges: "No changes allowed",
  formFillingAndSigning: "Filling in and signing allowed",
  formFillingSigningAndComments: "Filling in, signing and comments allowed",
};

export const CERTIFY_HINTS: Record<CertificationLevel, string> = {
  noChanges: "Any later change shows as not allowed. Use it for a finished document nobody should fill in.",
  formFillingAndSigning: "Others may fill in the form and sign it; anything else shows as not allowed.",
  formFillingSigningAndComments: "As above, and others may also add comments.",
};

export const VERDICT_LABELS: Record<ChangeVerdict, string> = {
  unchanged: "Unchanged",
  asIntended: "Changed as allowed",
  notAllowed: "Changed in ways not allowed",
  noLimits: "Changed since",
};

export const CHANGE_KIND_LABELS: Record<ChangeKind, string> = {
  fieldValue: "Answer",
  fieldDrawing: "Field drawing",
  signature: "Signature",
  documentTimestamp: "Document time stamp",
  formDefinition: "Form fields",
  annotation: "Comment",
  pageContent: "Page content",
  validationData: "Validation data",
  auditTrail: "Audit trail",
  metadata: "Document details",
  other: "Other",
};

export function noChoices(): SigningChoices {
  return { trustedTime: false, certify: null, lock: null, followSigningOrder: false, validationData: false, auditTrail: true };
}

/** the lock for what was chosen: none, every field, or the fields ticked. */
export function lockFor(kind: "none" | "all" | "chosen", fields: string[]): FieldLock | null {
  if (kind === "all") return { action: "all" };
  if (kind === "chosen" && fields.length > 0) return { action: "include", fields: [...fields] };
  return null;
}

/** why the choices can't be used yet, or null. */
export function choicesProblem(choices: SigningChoices, timestampServer: string, lockKind: "none" | "all" | "chosen"): string | null {
  if (choices.trustedTime && !timestampServer.trim()) return "Choose a time stamp server under Settings, Signing, or untick Add a Trusted Time.";
  if (lockKind === "chosen" && !choices.lock) return "Tick the fields to lock, or choose Lock None.";
  return null;
}

/** the "what will happen" line under Sign, with the extras chosen. */
export function signChoicesEffect(dirty: boolean, choices: SigningChoices, timestampServer: string): string {
  const parts: string[] = [];
  parts.push(dirty ? "Saves your changes, then signs the saved file" : "Signs the saved file");
  if (choices.certify) parts.push(`certifies it (${CERTIFY_LABELS[choices.certify].toLowerCase()})`);
  if (choices.lock) parts.push(choices.lock.action === "all" ? "locks every field" : `locks ${(choices.lock as { fields: string[] }).fields.length} fields`);
  if (choices.trustedTime) parts.push(`asks ${hostOf(timestampServer)} for a trusted time`);
  if (choices.validationData) parts.push("stores the certificates for later checking");
  if (choices.auditTrail) parts.push("records the signature in the audit trail");
  const list = parts.length > 1 ? `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}` : parts[0];
  return `${list}, then saves it again at once. Later changes are saved after the signature, which lists them.`;
}

/** the server's name for a sentence: "tsa.example.org", or the address as typed. */
export function hostOf(address: string): string {
  try {
    return new URL(address).host || address;
  } catch {
    return address || "the time stamp server";
  }
}

/** where a signature's time comes from, and the time: "Time from a time stamp server: 2026-11-12 10:00 UTC." */
export function timeLine(details: SignatureDetails, signedAt: string | null): string {
  const source = details.timeSource === "timestampAuthority" ? "Time from a time stamp server" : "Time from the signer’s computer";
  const time = details.timestamp?.time ?? signedAt;
  const server = details.timestamp?.authority ? ` (${details.timestamp.authority})` : "";
  return time ? `${source}${server}: ${time}.` : `${source}${server}.`;
}

/** one line per audit entry: what happened, in a few words. */
export function auditEventText(event: AuditEvent): string {
  switch (event.type) {
    case "created":
      return "Made in Letters";
    case "prepared":
      return event.note ? `Prepared: ${event.note}` : "Prepared for filling in";
    case "signed":
      return `Signed by ${event.signer} (${event.timeSource})`;
    case "saved":
      return "Saved after signing";
    case "timestamped":
      return `Time stamped by ${event.authority}`;
    case "note":
      return event.text;
  }
}
