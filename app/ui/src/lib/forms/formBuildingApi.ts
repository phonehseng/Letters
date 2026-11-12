// the calls for Prepare Form's building tools: tab order, groups, one
// field on several pages, export and default values, reset, and reset and
// print buttons. each change is one undo step in the working copy.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";
import type { Rect } from "../pageTools/pagePoints";
import type { FieldValue } from "./formsApi";

export type TabOrder = { type: "rows" } | { type: "columns" } | { type: "structure" } | { type: "manual"; fields: string[] };

export type ButtonAction = { type: "resetForm"; fields?: string[] | null } | { type: "print" };

export interface NewButton {
  name: string;
  page: number;
  rect: Rect;
  label: string;
  action: ButtonAction;
}

export function tabOrder(id: number, page: number): Promise<string[]> {
  return invoke("forms_tab_order", { id, page });
}

export function setTabOrder(id: number, page: number, order: TabOrder): Promise<DocumentState> {
  return invoke("forms_set_tab_order", { id, page, order });
}

export function groupFields(id: number, group: string, names: string[]): Promise<{ document: DocumentState; names: string[] }> {
  return invoke("forms_group_fields", { id, group, names });
}

export function duplicateAcrossPages(id: number, name: string, pages: number[]): Promise<DocumentState> {
  return invoke("forms_duplicate_across_pages", { id, name, pages });
}

export function setExportValue(id: number, name: string, button: number, value: string): Promise<DocumentState> {
  return invoke("forms_set_export_value", { id, name, button, value });
}

export function setDefaultValue(id: number, name: string, value: FieldValue | null): Promise<DocumentState> {
  return invoke("forms_set_default_value", { id, name, value });
}

export function resetForm(id: number, names: string[] | null): Promise<{ document: DocumentState; names: string[] }> {
  return invoke("forms_reset_form", { id, names });
}

export function addButton(id: number, button: NewButton): Promise<{ document: DocumentState; names: string[] }> {
  return invoke("forms_add_button", { id, button });
}
