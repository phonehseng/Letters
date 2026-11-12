// the office model (json from the shell) to an editor document and back.
//
// the rule: model -> editor -> model gives back exactly the same model when
// nothing was edited, preserved content, list numbering, style ids and
// tracked changes included. each node keeps its model object (minus its
// children) in `attrs.data`, so nothing has to be rebuilt from parts; see
// schema.ts. anything this file does not recognise is wrapped whole and
// unwrapped on the way back.

import { Fragment, Mark as PmMark, type Node as PmNode } from "prosemirror-model";
import type { Block, DocumentMeta, DocumentModel, Inline, Mark } from "./model";
import { schema } from "./schema";

type Data = Record<string, unknown>;

/** the key of a wrapped model object the schema has no node or mark for */
const RAW = "__raw";
/** a table cell or container whose block list was empty in the model */
const NO_BLOCKS = "__noBlocks";

function without(object: Data, ...keys: string[]): Data {
  const copy: Data = { ...object };
  for (const key of keys) delete copy[key];
  return copy;
}

// --- model to editor

export function modelToDoc(model: DocumentModel): PmNode {
  const { blocks, ...meta } = model;
  const nodes = blocksToNodes(blocks ?? []);
  return schema.node("doc", { meta, emptyBody: nodes.length === 0 }, nodes.length ? nodes : [emptyParagraph()]);
}

function emptyParagraph(): PmNode {
  return schema.node("paragraph", { data: {} });
}

export function blocksToNodes(blocks: Block[]): PmNode[] {
  return blocks.map(blockToNode);
}

function blockToNode(block: Block): PmNode {
  const data = without(block as Data, "type");
  switch (block.type) {
    case "paragraph":
      return schema.node("paragraph", { data: without(data, "content") }, inlinesToNodes(block.content as Inline[]));
    case "heading":
      return schema.node("heading", { level: block.level, data: without(data, "content", "level") }, inlinesToNodes(block.content as Inline[]));
    case "table":
      return tableToNode(data);
    case "pageBreak":
      return schema.node("page_break", { data });
    case "sectionBreak":
      return schema.node("section_break", { data });
    case "preserved":
      return schema.node("preserved_block", { data });
    case "textBox":
      return containerToNode("text_box", data);
    default:
      return schema.node("preserved_block", { data: { [RAW]: block } });
  }
}

function tableToNode(data: Data): PmNode {
  const rows = (data.rows as Data[]) ?? [];
  return schema.node(
    "table",
    { data: without(data, "rows") },
    rows.map((row) => schema.node("table_row", { data: without(row, "cells") }, ((row.cells as Data[]) ?? []).map(cellToNode))),
  );
}

function cellToNode(cell: Data): PmNode {
  const blocks = (cell.blocks as Block[]) ?? [];
  const data = without(cell, "blocks", "colSpan", "rowSpan");
  if (blocks.length === 0) data[NO_BLOCKS] = true;
  return schema.node(
    "table_cell",
    { colspan: cell.colSpan ?? 1, rowspan: cell.rowSpan ?? 1, data },
    blocks.length ? blocksToNodes(blocks) : [emptyParagraph()],
  );
}

/** a block that holds blocks (a text block): its own fields in data, its blocks as children. */
function containerToNode(name: string, data: Data): PmNode {
  const blocks = (data.blocks as Block[]) ?? [];
  const own = without(data, "blocks");
  if (blocks.length === 0) own[NO_BLOCKS] = true;
  return schema.node(name, { data: own }, blocks.length ? blocksToNodes(blocks) : [emptyParagraph()]);
}

function containerToModel(type: string, node: PmNode): Block {
  const data = node.attrs.data as Data;
  const blocks = nodesToBlocks(node.content);
  const keepEmpty = data[NO_BLOCKS] && blocks.length === 1 && isEmptyParagraph(blocks[0]);
  return { type, ...without(data, NO_BLOCKS), blocks: keepEmpty ? [] : blocks };
}

const INLINE_NODES: Record<string, string> = {
  tab: "tab",
  break: "hard_break",
  image: "image",
  noteReference: "note_ref",
  field: "field",
  bookmarkStart: "bookmark_start",
  bookmarkEnd: "bookmark_end",
  commentReference: "comment_ref",
  preservedRun: "preserved_run",
  preserved: "preserved_inline",
};

export function inlinesToNodes(inlines: Inline[] | undefined): PmNode[] {
  const nodes: PmNode[] = [];
  for (const inline of inlines ?? []) {
    const marks = (inline.marks ?? []).map(markToPm);
    if (inline.type === "text") {
      const text = String(inline.text ?? "");
      if (text) nodes.push(schema.text(text, marks));
      continue;
    }
    const name = INLINE_NODES[inline.type];
    if (name) nodes.push(schema.node(name, { data: without(inline as Data, "type", "marks") }, undefined, marks));
    else nodes.push(schema.node("preserved_inline", { data: { [RAW]: inline } }));
  }
  return nodes;
}

export function markToPm(mark: Mark): PmMark {
  const data = without(mark as Data, "type");
  if (mark.type === "insertion" || mark.type === "deletion") return schema.marks.change.create({ data: { kind: mark.type, ...data } });
  const type = schema.marks[mark.type];
  if (type) return type.create({ data });
  return schema.marks.preserved.create({ data: { [RAW]: mark } });
}

// --- editor to model

export function docToModel(doc: PmNode): DocumentModel {
  const meta = doc.attrs.meta as DocumentMeta;
  const blocks = nodesToBlocks(doc.content);
  const onlyEmpty = blocks.length === 1 && blocks[0].type === "paragraph" && isEmptyParagraph(blocks[0]);
  return { ...meta, blocks: doc.attrs.emptyBody && onlyEmpty ? [] : blocks } as DocumentModel;
}

function isEmptyParagraph(block: Block): boolean {
  return Object.keys(block).every((key) => key === "type" || key === "content") && (block.content as Inline[]).length === 0;
}

export function nodesToBlocks(fragment: Fragment): Block[] {
  const blocks: Block[] = [];
  fragment.forEach((node) => blocks.push(nodeToBlock(node)));
  return blocks;
}

function nodeToBlock(node: PmNode): Block {
  const data = node.attrs.data as Data;
  switch (node.type.name) {
    case "paragraph":
      return { type: "paragraph", ...data, content: nodesToInlines(node) };
    case "heading":
      return { type: "heading", level: node.attrs.level, ...data, content: nodesToInlines(node) };
    case "table":
      return { type: "table", ...data, rows: rowsOf(node) };
    case "page_break":
      return { type: "pageBreak", ...data };
    case "section_break":
      return { type: "sectionBreak", ...data };
    case "text_box":
      return containerToModel("textBox", node);
    default:
      if (data[RAW]) return data[RAW] as Block;
      return { type: "preserved", ...data };
  }
}

function rowsOf(table: PmNode): Data[] {
  const rows: Data[] = [];
  table.forEach((row) => {
    const cells: Data[] = [];
    row.forEach((cell) => cells.push(cellToModel(cell)));
    rows.push({ ...(row.attrs.data as Data), cells });
  });
  return rows;
}

function cellToModel(cell: PmNode): Data {
  const data = cell.attrs.data as Data;
  const blocks = nodesToBlocks(cell.content);
  const keepEmpty = data[NO_BLOCKS] && blocks.length === 1 && isEmptyParagraph(blocks[0]);
  return { colSpan: cell.attrs.colspan, rowSpan: cell.attrs.rowspan, ...without(data, NO_BLOCKS), blocks: keepEmpty ? [] : blocks };
}

const MODEL_INLINES: Record<string, string> = Object.fromEntries(Object.entries(INLINE_NODES).map(([model, node]) => [node, model]));

/** kinds of inline content whose model form has no `marks` field. */
const UNMARKED = new Set(["bookmarkStart", "bookmarkEnd", "commentReference", "preserved"]);

export function nodesToInlines(parent: PmNode): Inline[] {
  const inlines: Inline[] = [];
  parent.forEach((node) => {
    const marks = node.marks.map(markToModel);
    if (node.isText) {
      inlines.push(marks.length ? { type: "text", text: node.text ?? "", marks } : { type: "text", text: node.text ?? "" });
      return;
    }
    const data = node.attrs.data as Data;
    if (data[RAW]) {
      inlines.push(data[RAW] as Inline);
      return;
    }
    const type = MODEL_INLINES[node.type.name] ?? "preserved";
    const inline: Inline = { type, ...data };
    if (marks.length && !UNMARKED.has(type)) inline.marks = marks;
    inlines.push(inline);
  });
  return inlines;
}

export function markToModel(mark: PmMark): Mark {
  const data = mark.attrs.data as Data;
  if (mark.type.name === "change") return { type: String(data.kind), ...without(data, "kind") };
  if (mark.type.name === "preserved" && data[RAW]) return data[RAW] as Mark;
  return { type: mark.type.name, ...data };
}

/** blocks of a header, footer, note or comment as a small editor document. */
export function blocksToDoc(blocks: Block[]): PmNode {
  const nodes = blocksToNodes(blocks);
  return schema.node("doc", { meta: {}, emptyBody: nodes.length === 0 }, nodes.length ? nodes : [emptyParagraph()]);
}

export function docToBlocks(doc: PmNode): Block[] {
  return docToModel(doc).blocks;
}
