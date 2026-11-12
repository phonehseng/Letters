// every Fill & Sign call into the shell, typed, and the shapes of what
// comes back. the shell passes these to core/forms, whose rules apply:
// pages are counted from 0, positions are in points from the top left of
// the page as shown (y down), and every change to fields is one undo step
// in the working copy. signing is the exception: it signs the saved file.
//
// everything happens on this computer. nothing is sent anywhere.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog, save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { DocumentState } from "../api";
import type { Rect } from "../pageTools/pagePoints";
import type { ChangeVerdict, SigningChoices } from "./signingApi";

export type FieldKind = "text" | "multilineText" | "checkbox" | "radioGroup" | "dropdown" | "list" | "date" | "signature" | "button";
export type Alignment = "left" | "center" | "right";

export interface WidgetInfo {
  /** null when the widget isn't on any page */
  page: number | null;
  rect: Rect;
  /** a checkbox's or radio button's value when chosen */
  onValue: string | null;
}

export interface ChoiceOption {
  value: string;
  label: string;
}

export interface FieldInfo {
  /** the full name, parts joined with dots; what filling uses */
  name: string;
  kind: FieldKind;
  widgets: WidgetInfo[];
  value: string | null;
  selected: string[];
  checked: boolean | null;
  defaultValue: string | null;
  options: ChoiceOption[];
  required: boolean;
  readOnly: boolean;
  maxLength: number | null;
  comb: boolean;
  password: boolean;
  multiSelect: boolean;
  editable: boolean;
  tooltip: string | null;
  dateFormat: string | null;
  alignment: Alignment;
  /** 0 when the size follows the field */
  fontSize: number;
  signed: boolean;
}

export type FieldValue = { kind: "text"; value: string } | { kind: "checked"; value: boolean } | { kind: "selected"; value: string[] };

export type NewFieldKind =
  | { type: "text"; maxLength?: number | null; comb?: boolean }
  | { type: "multilineText" }
  | { type: "checkbox"; onValue?: string | null }
  | { type: "radioGroup"; buttons: { value: string; rect: Rect }[] }
  | { type: "dropdown"; options: ChoiceOption[]; editable?: boolean }
  | { type: "list"; options: ChoiceOption[]; multiSelect?: boolean }
  | { type: "date"; format?: string | null }
  | { type: "signature" };

export interface NewField {
  name: string;
  page: number;
  rect: Rect;
  kind: NewFieldKind;
  tooltip?: string | null;
  required?: boolean;
  readOnly?: boolean;
  fontSize?: number;
  alignment?: Alignment;
}

/** what update_field can change; leave out what stays. */
export interface FieldChanges {
  newName?: string;
  /** which widget `rect` and `page` are about */
  widget?: number;
  rect?: Rect;
  page?: number;
  options?: ChoiceOption[];
  required?: boolean;
  readOnly?: boolean;
  maxLength?: number;
  comb?: boolean;
  multiSelect?: boolean;
  tooltip?: string;
  fontSize?: number;
  alignment?: Alignment;
  dateFormat?: string;
}

export type DetectedKind = "text" | "multilineText" | "checkbox" | "radio" | "comb";

export interface DetectedField {
  kind: DetectedKind;
  rect: Rect;
  label: string | null;
  suggestedName: string;
  maxLength: number | null;
  group: number | null;
  confidence: number;
}

export interface PageProposals {
  page: number;
  fields: DetectedField[];
}

export interface CertificateSummary {
  subject: string;
  issuer: string;
  commonName: string | null;
  selfSigned: boolean;
  validFrom: string;
  validUntil: string;
  validWhenSigned: boolean | null;
  chain: string[];
}

export interface SignatureReport {
  field: string;
  signer: string | null;
  name: string | null;
  signedAt: string | null;
  reason: string | null;
  location: string | null;
  subFilter: string | null;
  digestMatches: boolean;
  signatureCorrect: boolean;
  valid: boolean;
  coversWholeFile: boolean;
  changedAfterSigning: boolean;
  laterUpdates: number;
  laterUpdatesAreSignatures: boolean;
  certificate: CertificateSummary | null;
  trust: "selfSigned" | "notChecked";
  summary: string;
  problems: string[];
}

export interface FormsOverview {
  fields: FieldInfo[];
  /** why the form can't be filled (an xfa form), in the engine's words */
  problem: string | null;
  /** "This form has a custom script Letters doesn’t run: a, b", when it has any */
  customScriptNote: string | null;
  signatures: SignatureReport[];
  /** for each signature, the engine's verdict on what changed after it */
  verdicts: (ChangeVerdict | null)[];
  /** the saved file is signed: changes are saved after its signatures */
  protected: boolean;
  trustNote: string;
}

export type PictureKind = "drawn" | "typed" | "image";

export interface SignaturePicture {
  id: string;
  kind: PictureKind;
  createdAt: number;
  width: number;
  height: number;
  /** a data: address for an <img> */
  picture: string;
}

export type SignaturePlacement =
  | { type: "existingField"; field: string }
  | { type: "newField"; page: number; rect: Rect; name?: string | null }
  | { type: "invisible"; name?: string | null };

export interface SignSettings {
  certificatePath: string;
  password: string;
  placement: SignaturePlacement;
  reason?: string | null;
  location?: string | null;
  signatureId?: string | null;
  showDetails?: boolean;
}

export interface CertificateRequest {
  name: string;
  email?: string | null;
  organization?: string | null;
  validDays: number;
  password: string;
  keyKind?: "rsa2048" | "p256";
}

export interface CertificateMade {
  path: string;
  subject: string;
  validFrom: string;
  validUntil: string;
  note: string;
}

export function formsOverview(id: number): Promise<FormsOverview> {
  return invoke("forms_overview", { id });
}

export function addFields(id: number, fields: NewField[]): Promise<{ document: DocumentState; names: string[] }> {
  return invoke("forms_add_fields", { id, fields });
}

export function updateField(id: number, name: string, changes: FieldChanges): Promise<{ document: DocumentState; name: string }> {
  return invoke("forms_update_field", { id, name, changes });
}

export function removeField(id: number, name: string): Promise<DocumentState> {
  return invoke("forms_remove_field", { id, name });
}

export function detectFields(id: number, pages: number[]): Promise<PageProposals[]> {
  return invoke("forms_detect_fields", { id, pages });
}

export function flattenForm(id: number, includeSignatures: boolean): Promise<{ document: DocumentState; flattened: string[] }> {
  return invoke("forms_flatten", { id, includeSignatures });
}

export function placeSignature(id: number, page: number, rect: Rect, signatureId: string): Promise<DocumentState> {
  return invoke("forms_place_signature", { id, page, rect, signatureId });
}

export function signaturePictures(): Promise<SignaturePicture[]> {
  return invoke("forms_signature_pictures");
}

export function keepSignaturePicture(png: Uint8Array, kind: PictureKind): Promise<SignaturePicture> {
  return invoke("forms_keep_signature_picture", { png: Array.from(png), kind });
}

export function deleteSignaturePicture(signatureId: string): Promise<void> {
  return invoke("forms_delete_signature_picture", { signatureId });
}

/** a chosen picture file as a data: address. */
export function readPicture(path: string): Promise<string> {
  return invoke("forms_read_picture", { path });
}

export function createCertificate(request: CertificateRequest, path: string): Promise<CertificateMade> {
  return invoke("forms_create_certificate", { request, path });
}

/** `choices` are the signing extras (signingApi.ts); leave out for a plain signature */
export function signDocument(id: number, settings: SignSettings, choices: SigningChoices | null = null): Promise<DocumentState> {
  return invoke("forms_sign", { id, settings, choices });
}

/** the file picker for a certificate file. */
export async function chooseCertificate(): Promise<string | null> {
  const chosen = await showOpenDialog({
    multiple: false,
    filters: [{ name: "Certificate files", extensions: ["p12", "pfx"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}

/** the save panel for a new certificate file. */
export async function chooseCertificateLocation(suggested: string): Promise<string | null> {
  const chosen = await showSaveDialog({
    defaultPath: suggested,
    filters: [{ name: "Certificate files", extensions: ["p12"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}
