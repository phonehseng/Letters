// the order Tab moves through a form, which fields are filled, and the
// words a screen reader hears on each one.
//
// a form's fields come from the engine in the order of its field tree.
// Tab follows the pages instead: page by page, and on each page in rows,
// top to bottom and left to right, which is how people read a form (and
// the order Letters gives new fields, with /Tabs /R). each field is one
// stop, at its first place in that order; a radio group is one stop
// whose buttons the arrow keys move between, as in any app.

import type { Rect } from "../pageTools/pagePoints";
import type { FieldInfo, PageTabOrder } from "./formsApi";

/** one place Tab stops: a field, at one of its widgets. */
export interface FieldStop {
  field: string;
  /** the widget's index in the field's list */
  widget: number;
  page: number;
  rect: Rect;
}

/** whether a field takes an answer typed or chosen by the person. */
export function isFillable(field: FieldInfo): boolean {
  return !field.readOnly && field.kind !== "signature" && field.kind !== "button";
}

/** two boxes are in one row when they overlap by half the smaller one's height. */
function sameRow(a: Rect, b: Rect): boolean {
  const overlap = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
  return overlap >= Math.min(a.height, b.height) / 2;
}

/** reading order on one page or across pages. */
export function readingOrder(a: { page: number; rect: Rect }, b: { page: number; rect: Rect }): number {
  if (a.page !== b.page) return a.page - b.page;
  if (sameRow(a.rect, b.rect)) return a.rect.x - b.rect.x;
  return a.rect.y - b.rect.y;
}

/** reading order down the columns first, for a page whose /Tabs is "C". */
function columnOrder(a: { rect: Rect }, b: { rect: Rect }): number {
  const overlap = Math.min(a.rect.x + a.rect.width, b.rect.x + b.rect.width) - Math.max(a.rect.x, b.rect.x);
  if (overlap >= Math.min(a.rect.width, b.rect.width) / 2) return a.rect.y - b.rect.y;
  return a.rect.x - b.rect.x;
}

/**
 * the order on one page as the file says it (iso 32000 12.5.1): rows for
 * "R", columns for "C", otherwise the page's own list, which Prepare Form ›
 * Order writes. Tab followed rows whatever the file said, so a tab order
 * set there worked in other apps but not in Letters.
 */
function pageOrder(order: PageTabOrder | undefined): (a: FieldStop, b: FieldStop) => number {
  if (!order || order.kind === "R") return readingOrder;
  if (order.kind === "C") return columnOrder;
  const rank = (stop: FieldStop) => {
    const index = order.fields.indexOf(stop.field);
    return index < 0 ? Number.MAX_SAFE_INTEGER : index;
  };
  return (a, b) => rank(a) - rank(b) || readingOrder(a, b);
}

/** every Tab stop of a form, in order: page by page, each page in its own order. */
export function tabStops(fields: FieldInfo[], orders: PageTabOrder[] = []): FieldStop[] {
  const stops: FieldStop[] = [];
  for (const field of fields) {
    if (!isFillable(field)) continue;
    const places = field.widgets
      .map((widget, index) => ({ field: field.name, widget: index, page: widget.page ?? -1, rect: widget.rect }))
      .filter((place) => place.page >= 0)
      .sort(readingOrder);
    if (places.length > 0) stops.push(places[0]);
  }
  return stops.sort((a, b) => a.page - b.page || pageOrder(orders.find((order) => order.page === a.page))(a, b));
}

/** the stop after (or before) the field named `current`, going round at the ends. */
export function stepStop(stops: FieldStop[], current: string | null, backwards = false): FieldStop | null {
  if (stops.length === 0) return null;
  const at = stops.findIndex((stop) => stop.field === current);
  if (at < 0) return backwards ? stops[stops.length - 1] : stops[0];
  const next = (at + (backwards ? -1 : 1) + stops.length) % stops.length;
  return stops[next];
}

/** whether a field has an answer. */
export function isFilled(field: FieldInfo): boolean {
  switch (field.kind) {
    case "checkbox":
      return field.checked === true;
    case "radioGroup":
      return field.value !== null && field.value !== "";
    case "dropdown":
    case "list":
      return field.selected.length > 0 && field.selected.some((value) => value !== "");
    default:
      return (field.value ?? "") !== "";
  }
}

/**
 * the next empty field after `current`, going round, or null when every
 * field has an answer. a checkbox left unticked counts as empty only when
 * it is required, since "no" is a real answer for a checkbox.
 */
export function nextEmptyStop(stops: FieldStop[], fields: FieldInfo[], current: string | null): FieldStop | null {
  const byName = new Map(fields.map((field) => [field.name, field]));
  const start = stops.findIndex((stop) => stop.field === current);
  for (let step = 1; step <= stops.length; step += 1) {
    const stop = stops[(start + step + stops.length) % stops.length];
    const field = byName.get(stop.field);
    if (field && needsAnswer(field)) return stop;
  }
  return null;
}

function needsAnswer(field: FieldInfo): boolean {
  if (field.kind === "checkbox") return field.required && !isFilled(field);
  return !isFilled(field);
}

/** "7 of 12 filled": fillable fields with an answer, of all fillable fields. */
export function formProgress(fields: FieldInfo[]): { filled: number; total: number; text: string } {
  const fillable = fields.filter(isFillable);
  const filled = fillable.filter(isFilled).length;
  return { filled, total: fillable.length, text: `${filled} of ${fillable.length} filled` };
}

/**
 * what a field is called for people: its tooltip (the form's own words),
 * or else the last part of its name with underscores as spaces.
 */
export function fieldLabel(field: Pick<FieldInfo, "name" | "tooltip">): string {
  const tooltip = field.tooltip?.trim();
  if (tooltip) return tooltip;
  const last = field.name.split(".").pop() ?? field.name;
  return last.replace(/[_]+/g, " ").trim() || field.name;
}

const KIND_WORDS: Partial<Record<FieldInfo["kind"], string>> = {
  multilineText: "text with several lines",
  checkbox: "checkbox",
  radioGroup: "choice",
  dropdown: "pop-up menu",
  list: "list",
  date: "date",
};

/** what a screen reader hears on a field: "Field 3 of 12, Student ID, required". */
export function stopAnnouncement(stops: FieldStop[], field: FieldInfo): string {
  const index = stops.findIndex((stop) => stop.field === field.name);
  const where = index >= 0 ? `Field ${index + 1} of ${stops.length}, ` : "";
  const kind = KIND_WORDS[field.kind];
  const parts = [`${where}${fieldLabel(field)}`];
  if (kind) parts.push(kind);
  if (field.required) parts.push("required");
  if (field.kind === "checkbox") parts.push(field.checked ? "ticked" : "not ticked");
  return parts.join(", ");
}
