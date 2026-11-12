// text blocks (the model's `textBox`) and the other commands the office
// engine runs on the page's objects (shell-core/src/writer/objects.rs).
//
// the engine answers with the whole model; `applyEngineModel` replaces
// only the top-level blocks that changed, in one transaction, so every
// command is one undo step and the rest of the document is left alone.
// the other functions translate between the editor's positions and the
// engine's: block indexes and character offsets.

import type { Node as PmNode } from "prosemirror-model";
import { NodeSelection, TextSelection, type EditorState, type Transaction } from "prosemirror-state";
import { blocksToNodes, docToModel } from "./convert";
import type { Block, DocumentModel } from "./model";

/** inline nodes the engine counts as no character at all (floating.rs split_at_character). */
const ZERO_WIDTH = new Set(["bookmark_start", "bookmark_end", "comment_ref"]);

/**
 * the engine's character offset for a position `parentOffset` into a
 * paragraph: text counts by characters (not utf-16 units), bookmarks and
 * comment markers count nothing, every other inline thing counts one.
 */
export function engineOffset(paragraph: PmNode, parentOffset: number): number {
  let count = 0;
  let pos = 0;
  for (let index = 0; index < paragraph.childCount && pos < parentOffset; index += 1) {
    const child = paragraph.child(index);
    if (child.isText) {
      const taken = (child.text ?? "").slice(0, parentOffset - pos);
      count += Array.from(taken).length;
    } else if (!ZERO_WIDTH.has(child.type.name)) count += 1;
    pos += child.nodeSize;
  }
  return count;
}

/** the editor's offset into a paragraph for the engine's character offset (the inverse). */
export function editorOffset(paragraph: PmNode, offset: number): number {
  let remaining = offset;
  let pos = 0;
  for (let index = 0; index < paragraph.childCount; index += 1) {
    const child = paragraph.child(index);
    if (remaining <= 0 && !ZERO_WIDTH.has(child.type.name)) return pos;
    if (child.isText) {
      const characters = Array.from(child.text ?? "");
      if (remaining < characters.length) return pos + characters.slice(0, remaining).join("").length;
      remaining -= characters.length;
    } else if (!ZERO_WIDTH.has(child.type.name)) remaining -= 1;
    pos += child.nodeSize;
  }
  return pos;
}

/** where the cursor is, in the engine's terms: the top-level paragraph and the offset in it. */
export function cursorInTopParagraph(state: EditorState): { block: number; offset: number } | null {
  const { $from } = state.selection;
  if ($from.depth !== 1 || !$from.parent.isTextblock) return null;
  return { block: $from.index(0), offset: engineOffset($from.parent, $from.parentOffset) };
}

/** the top-level blocks a selection covers, as a range (start, end exclusive). */
export function selectedBlockRange(state: EditorState): { start: number; end: number } {
  const { $from, $to } = state.selection;
  const start = $from.index(0);
  const end = Math.max(start + 1, $to.depth === 0 ? $to.index(0) : $to.index(0) + 1);
  return { start, end: Math.min(end, state.doc.childCount) };
}

/** the text block around the cursor (or selected), and its position. */
export function textBoxAt(state: EditorState): { node: PmNode; pos: number; index: number } | null {
  const selection = state.selection;
  if (selection instanceof NodeSelection && selection.node.type.name === "text_box") {
    return { node: selection.node, pos: selection.from, index: state.doc.resolve(selection.from).index(0) };
  }
  const { $from } = selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    if ($from.node(depth).type.name === "text_box") return { node: $from.node(depth), pos: $from.before(depth), index: $from.index(0) };
  }
  return null;
}

/**
 * the block boundary nearest to `y` (root coordinates): where a dissolved
 * text block's content goes back into the flow. `tops` has each top-level
 * block's top, NaN for blocks out of the flow (text blocks sit on the
 * page, not in the text); `end` is where the flow ends. boundary `i` is
 * "before block i"; it sits at the top of the next block in the flow.
 */
export function nearestBoundary(tops: number[], end: number, y: number): number {
  let best = 0;
  let distance = Infinity;
  for (let index = 0; index <= tops.length; index += 1) {
    const next = tops.slice(index).find((top) => !Number.isNaN(top));
    const at = next ?? end;
    const gap = Math.abs(at - y);
    if (gap < distance) {
      distance = gap;
      best = index;
    }
  }
  return best;
}

/** the y of boundary `index`, for drawing the insertion line. */
export function boundaryY(tops: number[], end: number, index: number): number {
  return tops.slice(index).find((top) => !Number.isNaN(top)) ?? end;
}

/** the boundary index counted after the text block at `box` is removed (the engine's `at`). */
export function boundaryWithoutBox(boundary: number, box: number): number {
  return boundary > box ? boundary - 1 : boundary;
}

/** keys in a fixed order, so two equal models stringify the same whichever wrote them. */
export function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(",")}]`;
  if (value && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, item]) => item !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    return `{${entries.map(([key, item]) => `${JSON.stringify(key)}:${stableJson(item)}`).join(",")}}`;
  }
  return JSON.stringify(value ?? null);
}

/** the range of top-level blocks that differ: blocks before `from` and after the ends are the same. */
export function blockDiff(before: Block[], after: Block[]): { from: number; toBefore: number; toAfter: number } | null {
  const a = before.map(stableJson);
  const b = after.map(stableJson);
  let from = 0;
  while (from < a.length && from < b.length && a[from] === b[from]) from += 1;
  if (from === a.length && from === b.length) return null;
  let endA = a.length;
  let endB = b.length;
  while (endA > from && endB > from && a[endA - 1] === b[endB - 1]) {
    endA -= 1;
    endB -= 1;
  }
  return { from, toBefore: endA, toAfter: endB };
}

/** the position where top-level block `index` starts. */
export function blockPos(doc: PmNode, index: number): number {
  let pos = 0;
  for (let i = 0; i < Math.min(index, doc.childCount); i += 1) pos += doc.child(i).nodeSize;
  return pos;
}

/**
 * one transaction that turns the editor's document into the engine's
 * answer: the changed blocks are replaced, the document's other parts
 * (styles, lists...) are set when they changed. null when nothing did.
 */
export function applyEngineModel(state: EditorState, next: DocumentModel): Transaction | null {
  const current = docToModel(state.doc);
  const diff = blockDiff(current.blocks, next.blocks ?? []);
  const { blocks: _old, ...oldMeta } = current;
  const { blocks: _new, ...newMeta } = next;
  void _old;
  void _new;
  const metaChanged = stableJson(oldMeta) !== stableJson(newMeta);
  if (!diff && !metaChanged) return null;
  const tr = state.tr;
  if (diff) {
    const from = blockPos(state.doc, diff.from);
    const to = blockPos(state.doc, diff.toBefore);
    const nodes = blocksToNodes((next.blocks ?? []).slice(diff.from, diff.toAfter));
    if (to > from) tr.replaceWith(from, to, nodes);
    else tr.insert(from, nodes);
  }
  if (metaChanged) tr.setDocAttribute("meta", { ...(state.doc.attrs.meta as object), ...newMeta });
  return tr;
}

/** puts the cursor at the start of top-level block `index` (inside it when it holds text). */
export function cursorIntoBlock(tr: Transaction, index: number): Transaction {
  const pos = blockPos(tr.doc, index);
  return tr.setSelection(TextSelection.near(tr.doc.resolve(Math.min(pos + 1, tr.doc.content.size))));
}

/** selects the inline node at (top-level block, child index), e.g. a picture after the engine moved it. */
export function selectInline(tr: Transaction, block: number, inline: number): Transaction {
  const paragraph = tr.doc.maybeChild(block);
  if (!paragraph || inline >= paragraph.childCount) return tr;
  let pos = blockPos(tr.doc, block) + 1;
  for (let index = 0; index < inline; index += 1) pos += paragraph.child(index).nodeSize;
  return tr.setSelection(NodeSelection.create(tr.doc, pos));
}

/** the engine's name for the picture at `pos`: its top-level block and its index in the paragraph. */
export function imageRef(doc: PmNode, pos: number): { type: "image"; block: number; inline: number } | null {
  const $pos = doc.resolve(pos);
  if ($pos.depth !== 1 || !$pos.parent.isTextblock) return null;
  return { type: "image", block: $pos.index(0), inline: $pos.index(1) };
}
