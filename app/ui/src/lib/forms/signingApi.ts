// the calls for signing extras, Verify and the audit trail, typed as
// core/forms reports them (see core/forms/README.md, "Signatures" and
// "Audit trail"). changes here are made on the saved file, like signing.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";
import type { SignatureReport } from "./formsApi";

export type CertificationLevel = "noChanges" | "formFillingAndSigning" | "formFillingSigningAndComments";

export type FieldLock = { action: "all" } | { action: "include"; fields: string[] } | { action: "exclude"; fields: string[] };

export interface SigningChoices {
  trustedTime: boolean;
  certify: CertificationLevel | null;
  lock: FieldLock | null;
  followSigningOrder: boolean;
  validationData: boolean;
  auditTrail: boolean;
}

export interface SigningNotes {
  /** said word for word before any time stamp request */
  timestampDisclosure: string;
  clockTimeNote: string;
  /** said word for word wherever the audit trail is */
  auditTrailNote: string;
  trustNote: string;
  /** the server chosen in Settings, empty for none */
  timestampServer: string;
  /** the name the audit trail records */
  actor: string;
}

export interface WorkflowStep {
  field: string;
  signed: boolean;
  signer: string | null;
  locks: FieldLock | null;
  isNext: boolean;
}

export interface TimestampInfo {
  time: string | null;
  authority: string | null;
  imprintMatches: boolean;
  signatureCorrect: boolean;
  certificateForTimestamping: boolean;
  problems: string[];
}

export interface SignatureDetails {
  field: string;
  kind: "approval" | "certification" | "documentTimestamp";
  timestamp: TimestampInfo | null;
  timeSource: "timestampAuthority" | "signerClock";
  timeNote: string;
}

export type ChangeKind =
  | "fieldValue"
  | "signature"
  | "documentTimestamp"
  | "formDefinition"
  | "annotation"
  | "pageContent"
  | "validationData"
  | "auditTrail"
  | "metadata"
  | "other";

export interface Change {
  kind: ChangeKind;
  field: string | null;
  page: number | null;
  description: string;
}

export interface RevisionReport {
  index: number;
  end: number;
  changes: Change[];
}

export type ChangeVerdict = "unchanged" | "asIntended" | "notAllowed" | "noLimits";

export interface SignatureChanges {
  field: string;
  revision: number | null;
  verdict: ChangeVerdict;
  sentence: string;
  changes: Change[];
  notAllowed: Change[];
}

export interface DocumentVerification {
  signatures: SignatureReport[];
  details: SignatureDetails[];
  certification: CertificationLevel | null;
  certificationNote: string | null;
  revisions: RevisionReport[];
  changes: SignatureChanges[];
  trustNote: string;
}

export type AuditEvent =
  | { type: "created" }
  | { type: "prepared"; note?: string | null }
  | { type: "signed"; field: string; signer: string; signingTime: string; timeSource: string; reason?: string | null; location?: string | null }
  | { type: "saved" }
  | { type: "timestamped"; authority: string }
  | { type: "note"; text: string };

export interface AuditEntry {
  sequence: number;
  event: AuditEvent;
  actor: string;
  time: string;
  hash: string;
}

export interface CheckedEntry {
  entry: AuditEntry;
  hashMatches: boolean;
  chainIntact: boolean;
  documentMatches: boolean;
  sealed: boolean;
}

export interface AuditTrailReport {
  verdict: "noTrail" | "intact" | "broken";
  entries: CheckedEntry[];
  problems: string[];
  summary: string;
  note: string;
}

export function signingNotes(): Promise<SigningNotes> {
  return invoke("forms_signing_notes");
}

export function signingWorkflow(id: number): Promise<WorkflowStep[]> {
  return invoke("forms_signing_workflow", { id });
}

export function setSigningOrder(id: number, fields: string[]): Promise<DocumentState> {
  return invoke("forms_set_signing_order", { id, fields });
}

export function addValidationData(id: number): Promise<DocumentState> {
  return invoke("forms_add_validation_data", { id });
}

export function verifyDocument(id: number): Promise<DocumentVerification> {
  return invoke("forms_verify_document", { id });
}

export function auditTrail(id: number): Promise<AuditTrailReport> {
  return invoke("forms_audit_trail", { id });
}

export function appendAuditEntry(id: number, event: AuditEvent): Promise<DocumentState> {
  return invoke("forms_append_audit_entry", { id, event });
}

export function addAuditPages(id: number): Promise<DocumentState> {
  return invoke("forms_add_audit_pages", { id });
}
