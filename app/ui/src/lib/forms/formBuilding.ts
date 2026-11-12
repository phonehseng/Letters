// the rules behind Prepare Form's building panes, kept apart from the
// components so they can be tested: moving a field in a tab order list,
// which fields can be grouped, where a new button goes, and the "what will
// happen" sentences.

import type { PageSize } from "../layout";
import type { Rect } from "../pageTools/pagePoints";
import type { FieldInfo } from "./formsApi";
import { fieldLabel } from "./tabOrder";

/** a list with one item moved from `from` to `to`; out of range changes nothing. */
export function moveItem<T>(list: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || to < 0 || from >= list.length || to >= list.length) return list;
  const moved = [...list];
  const [item] = moved.splice(from, 1);
  moved.splice(to, 0, item);
  return moved;
}

/**
 * only top-level fields can join a group (the engine refuses others), and
 * signature fields stay where signing expects them.
 */
export function groupable(fields: FieldInfo[]): FieldInfo[] {
  return fields.filter((field) => !field.name.includes(".") && field.kind !== "signature");
}

/** what a group name makes of a field: "street" in "address" is "address.street". */
export function groupedName(group: string, name: string): string {
  return `${group.trim()}.${name}`;
}

/** why a group name can't be used, or null. */
export function groupNameProblem(group: string, fields: FieldInfo[]): string | null {
  const name = group.trim();
  if (!name) return "Type a name for the group, such as address.";
  if (name.includes(".")) return "A group name can’t have a full stop in it.";
  if (fields.some((field) => field.name === name)) return `A field is already called “${name}”. Choose another group name.`;
  return null;
}

/** the usual size of a reset or print button, in points: about the size of a printed button label */
export const BUTTON_SIZE = { width: 96, height: 24 };

/** where a new button goes: the bottom right of the page, inside a margin of half an inch. */
export function buttonPlace(size: PageSize, index: number): Rect {
  const margin = 36;
  const gap = 8;
  const x = size.width - margin - BUTTON_SIZE.width * (index + 1) - gap * index;
  return { x: Math.max(margin, x), y: size.height - margin - BUTTON_SIZE.height, width: BUTTON_SIZE.width, height: BUTTON_SIZE.height };
}

/** the fields of a form that are on one page, in the order given. */
export function fieldsOnPage(order: string[], fields: FieldInfo[]): FieldInfo[] {
  return order.map((name) => fields.find((field) => field.name === name)).filter((field): field is FieldInfo => field !== undefined);
}

export function tabOrderEffect(kind: "rows" | "columns" | "structure" | "manual", page: number): string {
  const how = {
    rows: "across each row, rows from the top",
    columns: "down each column, columns from the left",
    structure: "in the order of the page’s tags (for tagged PDFs)",
    manual: "in the order of the list",
  }[kind];
  return `Tab will go through the fields on page ${page + 1} ${how}. Other PDF apps follow it too. Undo takes it back.`;
}

export function groupEffect(group: string, names: string[], fields: FieldInfo[]): string {
  if (names.length === 0) return "Choose the fields to put in the group.";
  const first = fields.find((field) => field.name === names[0]);
  const example = first ? ` “${first.name}” becomes “${groupedName(group || "group", first.name)}”.` : "";
  return `Puts ${names.length} ${names.length === 1 ? "field" : "fields"} under one name.${example} Answers and places stay the same.`;
}

export function resetEffect(field: FieldInfo | undefined): string {
  if (!field) return "Puts every field back to its default answer (empty when it has none). Undo takes it back.";
  return `Puts “${fieldLabel(field)}” back to its default answer. Undo takes it back.`;
}
