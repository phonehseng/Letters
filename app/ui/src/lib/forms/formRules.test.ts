// Prepare Form's rules: the draft read from the engine's model, turned
// back into it, and the sentences for what is missing.

import { describe, expect, it } from "vitest";
import type { FieldLogic } from "./formLogicApi";
import { changesFrom, draftFrom, emptyDraft, recalculatedSentence, ruleSummary } from "./formRules";

const plainField = { required: false, maxLength: null };

function logic(parts: Partial<FieldLogic>): FieldLogic {
  return { field: "total", calculation: null, format: null, validations: [], unsupported: [], ...parts };
}

describe("rules read from the file and written back", () => {
  it("a sum of fields with a money format and a range comes back unchanged", () => {
    const read = logic({
      calculation: { type: "aggregate", function: "sum", fields: ["price", "tax"] },
      format: { type: "number", decimals: 2, separator: "dotComma", negative: "parentheses", currency: " €", currencyBefore: false },
      validations: [{ type: "range", min: 0, max: 1000 }],
    });
    const result = changesFrom(draftFrom(read, plainField));
    expect(result).toEqual({ changes: { calculation: read.calculation, format: read.format, validations: read.validations } });
  });

  it("a formula, a date and a condition with its message come back unchanged", () => {
    const read = logic({
      calculation: { type: "formula", formula: "price * qty" },
      format: { type: "date", pattern: "yyyy-mm-dd" },
      validations: [{ type: "condition", condition: "value <= budget", message: "Over the budget." }],
    });
    expect(changesFrom(draftFrom(read, plainField))).toEqual({
      changes: { calculation: read.calculation, format: read.format, validations: read.validations },
    });
  });

  it("required and the most characters come from the field's own flags", () => {
    const draft = draftFrom(undefined, { required: true, maxLength: 8 });
    expect(changesFrom(draft)).toEqual({
      changes: { calculation: null, format: null, validations: [{ type: "required" }, { type: "maxLength", max: 8 }] },
    });
  });
});

describe("what is missing is said in a sentence", () => {
  it("a sum needs fields to add up", () => {
    const draft = { ...emptyDraft(), calculation: "sum" as const };
    expect(changesFrom(draft)).toEqual({ problem: "Choose the fields to work out the result from." });
  });

  it("a formula can't be empty", () => {
    expect(changesFrom({ ...emptyDraft(), calculation: "formula", formula: "  " })).toEqual({
      problem: "Type a formula, such as price * quantity.",
    });
  });

  it("a range takes numbers in order", () => {
    expect(changesFrom({ ...emptyDraft(), rangeMin: "ten" })).toEqual({ problem: "The range takes numbers, such as 0 and 100." });
    expect(changesFrom({ ...emptyDraft(), rangeMin: "10", rangeMax: "1" })).toEqual({
      problem: "The smallest allowed number is bigger than the largest.",
    });
  });

  it("a range and a condition can't both be kept, because a field has one check action", () => {
    const draft = { ...emptyDraft(), rangeMax: "5", condition: "value > 1" };
    expect(changesFrom(draft)).toEqual({ problem: "Choose a range or a condition, not both: a field keeps one check of that kind." });
  });

  it("the most characters is a whole number", () => {
    expect(changesFrom({ ...emptyDraft(), maxLength: "2.5" })).toEqual({ problem: "The most characters must be a whole number." });
  });
});

describe("plain words about rules", () => {
  it("summarises a field's rules in one line", () => {
    const read = logic({
      calculation: { type: "aggregate", function: "average", fields: ["a", "b"] },
      format: { type: "percent", decimals: 1, separator: "commaDot" },
      validations: [{ type: "required" }],
    });
    expect(ruleSummary(read)).toBe("Average of a, b · Percent · Required");
    expect(ruleSummary(undefined)).toBe("No rules");
  });

  it("says what a filled answer changed elsewhere, by the field's label", () => {
    const field = (name: string) => (name === "total" ? ({ name, tooltip: "Total due" } as never) : undefined);
    const said = recalculatedSentence(
      [
        { name: "total", value: "1234.5", shown: "1,234.50" },
        { name: "tax", value: "", shown: "" },
      ],
      field,
    );
    expect(said).toBe("Total due is now 1,234.50. tax is now empty.");
  });
});
