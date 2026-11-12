// a field added by hand: its default size, its name, and what the engine
// is asked to make for each kind. nothing here touches the screen.

import type { Rect } from "../pageTools/pagePoints";
import type { ChoiceOption, NewField } from "./formsApi";

export type AddableKind = "text" | "multilineText" | "checkbox" | "radioGroup" | "dropdown" | "date" | "signature";

export const ADDABLE_KINDS: { kind: AddableKind; label: string }[] = [
  { kind: "text", label: "Text" },
  { kind: "multilineText", label: "Text with Several Lines" },
  { kind: "checkbox", label: "Checkbox" },
  { kind: "radioGroup", label: "Radio Buttons" },
  { kind: "dropdown", label: "Pop-Up Menu" },
  { kind: "date", label: "Date" },
  { kind: "signature", label: "Signature" },
];

/** the size a field gets when placed with a click or from the keyboard, in points. */
export const NEW_FIELD_SIZE: Record<AddableKind, { width: number; height: number }> = {
  text: { width: 180, height: 22 },
  multilineText: { width: 240, height: 72 },
  checkbox: { width: 14, height: 14 },
  radioGroup: { width: 120, height: 16 },
  dropdown: { width: 150, height: 22 },
  date: { width: 110, height: 22 },
  signature: { width: 180, height: 48 },
};

/** the word a new field's name starts with: "Text 1", "Checkbox 2". */
const NAME_STEMS: Record<AddableKind, string> = {
  text: "Text",
  multilineText: "Text",
  checkbox: "Checkbox",
  radioGroup: "Choice",
  dropdown: "Menu",
  date: "Date",
  signature: "Signature",
};

/** a name no field has yet. dots join name parts in pdf, so there are none. */
export function suggestedName(kind: AddableKind, taken: string[]): string {
  const names = new Set(taken);
  for (let number = 1; ; number += 1) {
    const name = `${NAME_STEMS[kind]} ${number}`;
    if (!names.has(name)) return name;
  }
}

/** the choices typed one per line, without blanks or repeats. */
export function parseChoices(text: string): ChoiceOption[] {
  const seen = new Set<string>();
  const options: ChoiceOption[] = [];
  for (const line of text.split(/\r?\n/)) {
    const label = line.trim();
    if (!label || seen.has(label)) continue;
    seen.add(label);
    options.push({ value: label, label });
  }
  return options;
}

/**
 * the buttons of a radio group drawn as one rectangle: square buttons as
 * tall as the rectangle (at most 16 points), spread evenly along it.
 */
export function radioButtonRects(rect: Rect, count: number): Rect[] {
  const size = Math.min(16, rect.height, rect.width / Math.max(1, count));
  const gap = count > 1 ? (rect.width - size * count) / (count - 1) : 0;
  return Array.from({ length: count }, (_, index) => ({
    x: rect.x + index * (size + gap),
    y: rect.y + (rect.height - size) / 2,
    width: size,
    height: size,
  }));
}

/** what the engine is asked to add for a field drawn by hand. */
export function newFieldSpec(kind: AddableKind, page: number, rect: Rect, name: string, choices: ChoiceOption[]): NewField {
  const base = { name, page, rect };
  switch (kind) {
    case "multilineText":
      return { ...base, kind: { type: "multilineText" } };
    case "checkbox":
      return { ...base, kind: { type: "checkbox" } };
    case "radioGroup": {
      const options = choices.length >= 2 ? choices : [{ value: "Choice 1", label: "Choice 1" }, { value: "Choice 2", label: "Choice 2" }];
      const rects = radioButtonRects(rect, options.length);
      return { ...base, kind: { type: "radioGroup", buttons: options.map((option, index) => ({ value: option.value, rect: rects[index] })) } };
    }
    case "dropdown":
      return { ...base, kind: { type: "dropdown", options: choices.length > 0 ? choices : [{ value: "Choice 1", label: "Choice 1" }] } };
    case "date":
      return { ...base, kind: { type: "date", format: null } };
    case "signature":
      return { ...base, kind: { type: "signature" } };
    default:
      return { ...base, kind: { type: "text" } };
  }
}

/** whether a kind needs a list of choices before it is drawn. */
export function needsChoices(kind: AddableKind): boolean {
  return kind === "radioGroup" || kind === "dropdown";
}
