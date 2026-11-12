// the calls for form logic (filling through a field's format and checks,
// and setting its calculation, format and checks in Prepare Form), typed
// as core/forms' logic model reads them. see core/forms/README.md, "Form
// logic", for what each kind means and how it is written to the file.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";
import type { FieldValue } from "./formsApi";

export type AggregateFunction = "sum" | "product" | "average" | "minimum" | "maximum";

export type Calculation =
  | { type: "aggregate"; function: AggregateFunction; fields: string[] }
  | { type: "formula"; formula: string }
  | { type: "script"; source: string };

export type SeparatorStyle = "commaDot" | "dot" | "dotComma" | "comma" | "apostropheDot";
export type NegativeStyle = "minusSign" | "red" | "parentheses" | "redParentheses";
export type SpecialFormat = "zipCode" | "zipPlus4" | "phone" | "socialSecurity";

export type FieldFormat =
  | { type: "number"; decimals: number; separator: SeparatorStyle; negative: NegativeStyle; currency: string; currencyBefore: boolean }
  | { type: "percent"; decimals: number; separator: SeparatorStyle }
  | { type: "date"; pattern: string }
  | { type: "time"; pattern: string }
  | { type: "special"; kind: SpecialFormat }
  | { type: "mask"; mask: string };

export type Validation =
  | { type: "range"; min: number | null; max: number | null }
  | { type: "required" }
  | { type: "maxLength"; max: number }
  | { type: "condition"; condition: string; message: string | null }
  | { type: "script"; source: string };

export interface UnsupportedScript {
  trigger: "calculate" | "format" | "keystroke" | "validate";
  excerpt: string;
  reason: string;
}

export interface FieldLogic {
  field: string;
  calculation: Calculation | null;
  format: FieldFormat | null;
  validations: Validation[];
  unsupported: UnsupportedScript[];
}

/** a problem with the calculations as a whole; every kind carries a sentence */
export type CalculationIssue = { type: string; message: string; field?: string; fields?: string[] };

export interface FormLogicView {
  fields: FieldLogic[];
  calculationOrder: string[];
  issues: CalculationIssue[];
  customScriptNote: string | null;
  fieldsWithCustomScripts: string[];
  /** why there is no submit button, in the engine's words */
  noSubmitNote: string;
  /** why fields never show or hide depending on answers */
  conditionalVisibilityNote: string;
}

export interface FieldLogicChanges {
  calculation: Calculation | null;
  format: FieldFormat | null;
  validations: Validation[];
}

export interface ChangedField {
  name: string;
  /** as stored in the file: 1234.5, a percentage as 0.125 */
  value: string;
  /** as the field shows it: 1,234.50 */
  shown: string;
}

export function formLogic(id: number): Promise<FormLogicView> {
  return invoke("forms_logic", { id });
}

/** fills a field through its format and checks, then works out every calculation again. */
export function fillField(id: number, name: string, value: FieldValue): Promise<{ document: DocumentState; changed: ChangedField[] }> {
  return invoke("forms_fill_field", { id, name, value });
}

export function setFieldLogic(id: number, name: string, changes: FieldLogicChanges): Promise<{ document: DocumentState; logic: FieldLogic }> {
  return invoke("forms_set_field_logic", { id, name, changes });
}

/** works every total out again, for a form whose totals are out of date. */
export function recalculate(id: number): Promise<{ document: DocumentState; changed: ChangedField[] }> {
  return invoke("forms_recalculate", { id });
}
