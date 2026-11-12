// the items of the toolbar's four menus. each item names its excel key,
// so the menu teaches the shortcut while it is used.

import type { PullDownItem } from "../writer/PullDown.svelte";
import type { SheetsContext } from "./gridActions";
import { clearFilter, deleteLines, freeze, hideLines, insertLines, setFormat, sortBy, toggleStyle } from "./sheetActions";
import { clampRange } from "./selection";
import { book, type FriendlyFormat } from "./sheetsApi";

export function formatItems(context: SheetsContext, formats: FriendlyFormat[], presets: string[]): PullDownItem[] {
  const align = (horizontal: string | null, label: string): PullDownItem => ({ label, run: () => setFormat(context, { change: "align", horizontal }, label) });
  return [
    { label: "Bold", shortcut: "Ctrl B", run: () => toggleStyle(context, "bold") },
    { label: "Italic", shortcut: "Ctrl I", run: () => toggleStyle(context, "italic") },
    { label: "Underline", shortcut: "Ctrl U", run: () => toggleStyle(context, "underline") },
    { label: "Strikethrough", shortcut: "Ctrl 5", run: () => toggleStyle(context, "strike") },
    { ...align("left", "Align Left"), separatorBefore: true },
    align("center", "Align Center"),
    align("right", "Align Right"),
    { label: "Wrap Text", title: "Long text goes onto more lines inside the cell", run: () => setFormat(context, { change: "wrap", on: true }, "Wrap Text") },
    ...formats.map((format, index) => ({
      label: `${format.name} — ${format.example}`,
      title: `Shows numbers as ${format.example}. The value stays the same.`,
      separatorBefore: index === 0,
      run: () => setFormat(context, { change: "numberFormat", code: format.code }, format.name),
    })),
    { label: "General", shortcut: "Ctrl Shift ~", run: () => setFormat(context, { change: "numberFormat", code: "General" }, "General") },
    ...presets.map((name, index) => ({ label: name, separatorBefore: index === 0, title: "A cell style, as in Excel's gallery", run: () => setFormat(context, { change: "preset", name }, name) })),
    {
      label: "Clear Formats",
      separatorBefore: true,
      title: "Back to plain cells; the values stay",
      run: () => void context.session.apply(context.view.selection.ranges.map((range) => book({ op: "clear", sheet: context.view.sheet, range: clampRange(range, context.view.usedRows(), context.view.usedCols()), what: "formats" })), "Clear Formats"),
    },
  ];
}

export function sortItems(context: SheetsContext, openFilter: () => void): PullDownItem[] {
  const filtered = !!context.view.info?.filter;
  return [
    { label: "Sort A to Z", title: "Smallest to largest, by the active cell's column", run: () => void sortBy(context, false) },
    { label: "Sort Z to A", title: "Largest to smallest, by the active cell's column", run: () => void sortBy(context, true) },
    { label: "Filter…", shortcut: "Ctrl Shift L", separatorBefore: true, run: openFilter },
    { label: "Clear Filter", disabled: !filtered, run: () => clearFilter(context) },
  ];
}

export function lineItems(context: SheetsContext): PullDownItem[] {
  return [
    { label: "Insert Rows Above", shortcut: "Ctrl Shift +", run: () => insertLines(context, "rows") },
    { label: "Insert Rows Below", run: () => insertLines(context, "rows", true) },
    { label: "Insert Columns Left", run: () => insertLines(context, "columns") },
    { label: "Insert Columns Right", run: () => insertLines(context, "columns", true) },
    { label: "Delete Rows", shortcut: "Ctrl −", separatorBefore: true, run: () => deleteLines(context, "rows") },
    { label: "Delete Columns", run: () => deleteLines(context, "columns") },
    { label: "Hide Rows", shortcut: "Ctrl 9", separatorBefore: true, run: () => hideLines(context, "rows", true) },
    { label: "Show Rows", shortcut: "Ctrl Shift 9", run: () => hideLines(context, "rows", false) },
    { label: "Hide Columns", shortcut: "Ctrl 0", run: () => hideLines(context, "columns", true) },
    { label: "Show Columns", shortcut: "Ctrl Shift 0", run: () => hideLines(context, "columns", false) },
  ];
}

export function freezeItems(context: SheetsContext): PullDownItem[] {
  const info = context.view.info;
  const frozen = (info?.frozenRows ?? 0) > 0 || (info?.frozenCols ?? 0) > 0;
  return [
    { label: "Freeze Top Row", run: () => freeze(context, "topRow") },
    { label: "Freeze First Column", run: () => freeze(context, "firstColumn") },
    { label: "Freeze Above and Left of the Active Cell", title: "Rows above and columns left of the active cell stay in view", run: () => freeze(context, "selection") },
    { label: "Unfreeze", disabled: !frozen, separatorBefore: true, run: () => freeze(context, "none") },
  ];
}
