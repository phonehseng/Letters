// Prepare Form's rules for one field, as the editor holds them: a draft
// with one plain setting per box, turned into the engine's model
// (formLogicApi.ts) and back. the engine writes the model as acrobat's
// standard functions, so other pdf apps run the same rules.
//
// a draft can be incomplete while it is typed; `changesFrom` says in one
// sentence what is still missing instead of sending it.

import type { FieldInfo } from "./formsApi";
import type {
  AggregateFunction,
  Calculation,
  ChangedField,
  FieldFormat,
  FieldLogic,
  FieldLogicChanges,
  NegativeStyle,
  SeparatorStyle,
  SpecialFormat,
  Validation,
} from "./formLogicApi";
import { fieldLabel } from "./tabOrder";

export type CalculationChoice = "none" | AggregateFunction | "formula" | "script";
export type FormatChoice = "none" | FieldFormat["type"];

export interface RuleDraft {
  calculation: CalculationChoice;
  calculationFields: string[];
  formula: string;
  /** a script in the safe subset, kept as it is (it can't be edited as a formula) */
  calculationScript: string;
  format: FormatChoice;
  decimals: number;
  separator: SeparatorStyle;
  negative: NegativeStyle;
  currency: string;
  currencyBefore: boolean;
  datePattern: string;
  timePattern: string;
  special: SpecialFormat;
  mask: string;
  required: boolean;
  /** empty for no limit */
  maxLength: string;
  rangeMin: string;
  rangeMax: string;
  condition: string;
  conditionMessage: string;
  /** a check script in the safe subset, kept as it is */
  checkScript: string;
}

export const CALCULATION_LABELS: Record<CalculationChoice, string> = {
  none: "None",
  sum: "Sum",
  product: "Product",
  average: "Average",
  minimum: "Smallest",
  maximum: "Largest",
  formula: "Formula",
  script: "Script",
};

export const FORMAT_LABELS: Record<FormatChoice, string> = {
  none: "None",
  number: "Number",
  percent: "Percent",
  date: "Date",
  time: "Time",
  special: "Special",
  mask: "Pattern",
};

export const SEPARATOR_LABELS: Record<SeparatorStyle, string> = {
  commaDot: "1,234.56",
  dot: "1234.56",
  dotComma: "1.234,56",
  comma: "1234,56",
  apostropheDot: "1'234.56",
};

export const NEGATIVE_LABELS: Record<NegativeStyle, string> = {
  minusSign: "-1,234.56",
  red: "1,234.56 in red",
  parentheses: "(1,234.56)",
  redParentheses: "(1,234.56) in red",
};

export const SPECIAL_LABELS: Record<SpecialFormat, string> = {
  zipCode: "ZIP code (12345)",
  zipPlus4: "ZIP+4 (12345-6789)",
  phone: "Phone ((123) 456-7890)",
  socialSecurity: "Social Security (123-45-6789)",
};

export const TIME_PATTERNS = ["HH:MM", "h:MM tt", "HH:MM:ss", "h:MM:ss tt"];

export function emptyDraft(): RuleDraft {
  return {
    calculation: "none",
    calculationFields: [],
    formula: "",
    calculationScript: "",
    format: "none",
    decimals: 2,
    separator: "commaDot",
    negative: "minusSign",
    currency: "",
    currencyBefore: true,
    datePattern: "dd/mm/yyyy",
    timePattern: "HH:MM",
    special: "phone",
    mask: "",
    required: false,
    maxLength: "",
    rangeMin: "",
    rangeMax: "",
    condition: "",
    conditionMessage: "",
    checkScript: "",
  };
}

/** the editor's draft for a field's logic as the engine read it. */
export function draftFrom(logic: FieldLogic | undefined, field: Pick<FieldInfo, "required" | "maxLength">): RuleDraft {
  const draft = emptyDraft();
  draft.required = field.required;
  draft.maxLength = field.maxLength ? String(field.maxLength) : "";
  if (!logic) return draft;
  readCalculation(draft, logic.calculation);
  readFormat(draft, logic.format);
  for (const validation of logic.validations) readValidation(draft, validation);
  return draft;
}

function readCalculation(draft: RuleDraft, calculation: Calculation | null) {
  if (!calculation) return;
  if (calculation.type === "aggregate") {
    draft.calculation = calculation.function;
    draft.calculationFields = [...calculation.fields];
  } else if (calculation.type === "formula") {
    draft.calculation = "formula";
    draft.formula = calculation.formula;
  } else {
    draft.calculation = "script";
    draft.calculationScript = calculation.source;
  }
}

function readFormat(draft: RuleDraft, format: FieldFormat | null) {
  if (!format) return;
  draft.format = format.type;
  switch (format.type) {
    case "number":
      Object.assign(draft, { decimals: format.decimals, separator: format.separator, negative: format.negative });
      Object.assign(draft, { currency: format.currency, currencyBefore: format.currencyBefore });
      break;
    case "percent":
      Object.assign(draft, { decimals: format.decimals, separator: format.separator });
      break;
    case "date":
      draft.datePattern = format.pattern;
      break;
    case "time":
      draft.timePattern = format.pattern;
      break;
    case "special":
      draft.special = format.kind;
      break;
    case "mask":
      draft.mask = format.mask;
      break;
  }
}

function readValidation(draft: RuleDraft, validation: Validation) {
  switch (validation.type) {
    case "required":
      draft.required = true;
      break;
    case "maxLength":
      draft.maxLength = String(validation.max);
      break;
    case "range":
      draft.rangeMin = validation.min === null ? "" : String(validation.min);
      draft.rangeMax = validation.max === null ? "" : String(validation.max);
      break;
    case "condition":
      draft.condition = validation.condition;
      draft.conditionMessage = validation.message ?? "";
      break;
    case "script":
      draft.checkScript = validation.source;
      break;
  }
}

/** the engine's changes for a draft, or the sentence saying what is missing. */
export function changesFrom(draft: RuleDraft): { changes: FieldLogicChanges } | { problem: string } {
  const calculation = calculationOf(draft);
  if (typeof calculation === "string") return { problem: calculation };
  const format = formatOf(draft);
  if (typeof format === "string") return { problem: format };
  const validations = validationsOf(draft);
  if (typeof validations === "string") return { problem: validations };
  return { changes: { calculation, format, validations } };
}

function calculationOf(draft: RuleDraft): Calculation | null | string {
  switch (draft.calculation) {
    case "none":
      return null;
    case "formula":
      return draft.formula.trim() ? { type: "formula", formula: draft.formula.trim() } : "Type a formula, such as price * quantity.";
    case "script":
      return { type: "script", source: draft.calculationScript };
    default:
      if (draft.calculationFields.length === 0) return "Choose the fields to work out the result from.";
      return { type: "aggregate", function: draft.calculation, fields: [...draft.calculationFields] };
  }
}

function formatOf(draft: RuleDraft): FieldFormat | null | string {
  switch (draft.format) {
    case "none":
      return null;
    case "number":
      return {
        type: "number",
        decimals: draft.decimals,
        separator: draft.separator,
        negative: draft.negative,
        currency: draft.currency,
        currencyBefore: draft.currencyBefore,
      };
    case "percent":
      return { type: "percent", decimals: draft.decimals, separator: draft.separator };
    case "date":
      return draft.datePattern.trim() ? { type: "date", pattern: draft.datePattern.trim() } : "Type how dates are written, such as dd/mm/yyyy.";
    case "time":
      return { type: "time", pattern: draft.timePattern };
    case "special":
      return { type: "special", kind: draft.special };
    case "mask":
      return draft.mask.trim() ? { type: "mask", mask: draft.mask } : "Type the pattern, such as 999-AAA (9 a digit, A a letter).";
  }
}

function validationsOf(draft: RuleDraft): Validation[] | string {
  const validations: Validation[] = [];
  if (draft.required) validations.push({ type: "required" });
  const limit = optionalNumber(draft.maxLength);
  if (limit === "bad" || (limit !== null && (limit < 1 || !Number.isInteger(limit)))) return "The most characters must be a whole number.";
  if (limit !== null) validations.push({ type: "maxLength", max: limit });
  const range = rangeOf(draft);
  if (typeof range === "string") return range;
  const condition = draft.condition.trim();
  const kinds = [range !== null, condition !== "", draft.checkScript !== ""].filter(Boolean).length;
  // a pdf field has one validate action, so the engine keeps one of these
  if (kinds > 1) return "Choose a range or a condition, not both: a field keeps one check of that kind.";
  if (range) validations.push(range);
  if (condition) validations.push({ type: "condition", condition, message: draft.conditionMessage.trim() || null });
  if (draft.checkScript) validations.push({ type: "script", source: draft.checkScript });
  return validations;
}

function rangeOf(draft: RuleDraft): Validation | null | string {
  const min = optionalNumber(draft.rangeMin);
  const max = optionalNumber(draft.rangeMax);
  if (min === "bad" || max === "bad") return "The range takes numbers, such as 0 and 100.";
  if (min === null && max === null) return null;
  if (min !== null && max !== null && min > max) return "The smallest allowed number is bigger than the largest.";
  return { type: "range", min, max };
}

function optionalNumber(text: string): number | null | "bad" {
  const trimmed = text.trim();
  if (!trimmed) return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : "bad";
}

/** one line about a field's rules, for the field list in Prepare Form. */
export function ruleSummary(logic: FieldLogic | undefined): string {
  if (!logic) return "No rules";
  const parts: string[] = [];
  const calculation = logic.calculation;
  if (calculation?.type === "aggregate") parts.push(`${CALCULATION_LABELS[calculation.function]} of ${calculation.fields.join(", ")}`);
  else if (calculation?.type === "formula") parts.push(`= ${calculation.formula}`);
  else if (calculation) parts.push("Calculated by a script");
  if (logic.format) parts.push(FORMAT_LABELS[logic.format.type]);
  for (const validation of logic.validations) parts.push(validationLabel(validation));
  if (logic.unsupported.length > 0) parts.push("Has a script Letters doesn’t run");
  return parts.length > 0 ? parts.join(" · ") : "No rules";
}

function validationLabel(validation: Validation): string {
  switch (validation.type) {
    case "required":
      return "Required";
    case "maxLength":
      return `At most ${validation.max} characters`;
    case "range":
      return `From ${validation.min ?? "any"} to ${validation.max ?? "any"}`;
    case "condition":
      return `Must hold: ${validation.condition}`;
    case "script":
      return "Checked by a script";
  }
}

/** what a filled answer changed elsewhere, for screen readers: "Total is now 1,234.50." */
export function recalculatedSentence(changed: ChangedField[], field: (name: string) => FieldInfo | undefined): string {
  const lines = changed.map((each) => {
    const info = field(each.name);
    const label = info ? fieldLabel(info) : each.name;
    return each.shown ? `${label} is now ${each.shown}.` : `${label} is now empty.`;
  });
  return lines.join(" ");
}
