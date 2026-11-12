// changes to the table around the cursor, as transactions the session runs:
// the table's own model data, each selected cell's, and its header row.

import type { EditorState, Transaction } from "prosemirror-state";

type Data = Record<string, unknown>;

const isCell = (name: string) => name === "table_cell" || name === "table_header";

/** the table holding the cursor, and its position, or null. */
function tableAround(state: EditorState): { node: import("prosemirror-model").Node; pos: number } | null {
  const here = state.selection.$from;
  for (let depth = here.depth; depth > 0; depth -= 1) {
    const node = here.node(depth);
    if (node.type.name === "table") return { node, pos: here.before(depth) };
  }
  return null;
}

/** changes the table's own data (style, width, borders...). */
export function updateTable(state: EditorState, change: (data: Data) => Data): Transaction | null {
  const table = tableAround(state);
  if (!table) return null;
  return state.tr.setNodeMarkup(table.pos, undefined, { data: change({ ...(table.node.attrs.data as Data) }) });
}

/** changes each selected cell's data, or the cell holding the cursor when none is selected. */
export function updateCells(state: EditorState, change: (data: Data) => Data): Transaction {
  const tr = state.tr;
  const { from, to } = state.selection;
  let touched = false;
  state.doc.nodesBetween(Math.min(from, to), Math.max(from, to), (node, pos) => {
    if (!isCell(node.type.name)) return true;
    tr.setNodeMarkup(pos, undefined, { ...node.attrs, data: change({ ...(node.attrs.data as Data) }) });
    touched = true;
    return false;
  });
  if (touched) return tr;
  const here = state.selection.$from;
  for (let depth = here.depth; depth > 0; depth -= 1) {
    const node = here.node(depth);
    if (!isCell(node.type.name)) continue;
    tr.setNodeMarkup(here.before(depth), undefined, { ...node.attrs, data: change({ ...(node.attrs.data as Data) }) });
    break;
  }
  return tr;
}

/** marks the table's first row as a header row (repeated on each page in Word), or not. */
export function setHeaderRow(state: EditorState, on: boolean): Transaction | null {
  const table = tableAround(state);
  const row = table?.node.firstChild;
  if (!table || !row) return null;
  return state.tr.setNodeMarkup(table.pos + 1, undefined, { data: { ...(row.attrs.data as Data), header: on } });
}
