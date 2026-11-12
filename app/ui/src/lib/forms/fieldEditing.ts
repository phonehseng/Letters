// what the on-page editor of a field holds while someone types, and the
// answer it hands to the engine.
//
// typing is kept in the editor and sent once, when the field is left
// (Tab, Enter, a click elsewhere), so a whole answer is one undo step
// rather than one per letter. nothing here touches the screen.

import type { Rect } from "../pageTools/pagePoints";
import type { FieldInfo, FieldValue } from "./formsApi";

/** which on-page editor a field gets. */
export type EditorKind = "text" | "multiline" | "comb" | "date" | "dropdown" | "list" | "toggle" | "none";

export function editorKind(field: FieldInfo): EditorKind {
  if (field.readOnly) return "none";
  switch (field.kind) {
    case "text":
      return field.comb && field.maxLength ? "comb" : "text";
    case "multilineText":
      return "multiline";
    case "date":
      return "date";
    case "dropdown":
      return "dropdown";
    case "list":
      return "list";
    case "checkbox":
    case "radioGroup":
      return "toggle";
    default:
      return "none";
  }
}

/** the words in the editor when it opens: the field's current answer. */
export function startingText(field: FieldInfo): string {
  return field.value ?? "";
}

/** typing beyond the field's limit is cut, so the engine never has to refuse it. */
export function withinLimit(text: string, maxLength: number | null): string {
  if (!maxLength || maxLength <= 0) return text;
  return [...text].slice(0, maxLength).join("");
}

/** whether the editor's text differs from the field's answer. */
export function textChanged(field: FieldInfo, text: string): boolean {
  return text !== startingText(field);
}

export function textAnswer(text: string): FieldValue {
  return { kind: "text", value: text };
}

/**
 * what a click or Space does to a checkbox or radio button: a checkbox
 * flips; a radio button chooses its own value (choosing it again keeps it
 * chosen, as radio buttons do everywhere).
 */
export function toggledAnswer(field: FieldInfo, widget: number): FieldValue | null {
  if (field.kind === "checkbox") return { kind: "checked", value: !field.checked };
  if (field.kind === "radioGroup") {
    const value = field.widgets[widget]?.onValue;
    if (!value || field.value === value) return null;
    return { kind: "text", value };
  }
  return null;
}

/** whether one widget of a checkbox or radio group shows as chosen. */
export function widgetIsOn(field: FieldInfo, widget: number): boolean {
  if (field.kind === "checkbox") return field.checked === true;
  if (field.kind === "radioGroup") {
    const value = field.widgets[widget]?.onValue;
    return value !== null && value !== undefined && field.value === value;
  }
  return false;
}

/** the radio button the arrow keys move to, going round the group. */
export function neighbourWidget(field: FieldInfo, widget: number, key: string): number | null {
  const step = key === "ArrowDown" || key === "ArrowRight" ? 1 : key === "ArrowUp" || key === "ArrowLeft" ? -1 : 0;
  if (step === 0 || field.kind !== "radioGroup" || field.widgets.length < 2) return null;
  return (widget + step + field.widgets.length) % field.widgets.length;
}

export function choiceAnswer(values: string[]): FieldValue {
  return { kind: "selected", value: values };
}

/** the engine's line spacing, as a share of the text size (LINE_SPACING in core/forms) */
const LINE_SPACING = 1.15;

/** the text size the editor uses, in points, close to what the engine will draw. */
export function editorFontSize(field: FieldInfo, rect: Rect): number {
  if (field.fontSize > 0) return field.fontSize;
  if (field.kind === "multilineText") return Math.min(12, Math.max(4, rect.height - 4));
  // the engine fits a single line to the box's height (less a border), between 4 and 24 points
  const byHeight = (rect.height - 4) / LINE_SPACING;
  if (field.comb && field.maxLength) return Math.min(24, Math.max(4, Math.min(byHeight, (rect.width / field.maxLength) * 1.4)));
  return Math.min(24, Math.max(4, byHeight));
}

/**
 * the answer as the engine takes it. the engine refuses line breaks
 * (they aren't in winansi, the standard fonts' encoding), even in a
 * field with several lines, so they are sent as spaces: the engine
 * wraps the words to the box anyway.
 */
export function textForEngine(field: FieldInfo, text: string): string {
  return field.kind === "multilineText" ? text.replace(/\r\n|\r|\n/g, " ") : text;
}

/** how a comb field's characters sit one per box: a monospaced font spaced to the box width. */
export function combSpacing(rect: Rect, cells: number, fontSize: number): { letterSpacing: number; paddingLeft: number } {
  // a monospaced font's characters are 0.6 of its size wide
  const cell = rect.width / Math.max(1, cells);
  const glyph = fontSize * 0.6;
  return { letterSpacing: cell - glyph, paddingLeft: (cell - glyph) / 2 };
}

/** today in the format the engine always accepts, from a date picker's value. */
export function isoDate(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/**
 * a hint for a date field: its own pattern, so people type what the form
 * expects ("dd/mm/yyyy"), and that 2026-11-11 also works.
 */
export function dateHint(field: FieldInfo): string {
  return field.dateFormat ? `Type it as ${field.dateFormat}, or like 2026-11-11.` : "Type it like 2026-11-11.";
}
