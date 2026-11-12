// the editing commands every control of the writer uses: styles, marks,
// paragraph formatting, lists, indents, inserting things, and the page.
//
// each one takes the editor state and returns a transaction (or null when
// it does not apply), so toolbar buttons, menu commands, shortcuts and the
// inspector all do exactly the same thing, and each change is one undo step.
// nothing here touches the screen, so it is all unit tested.

import type { Mark as PmMark, MarkType, Node as PmNode } from "prosemirror-model";
import { NodeSelection, TextSelection, type EditorState, type Transaction } from "prosemirror-state";
import { inlinesToNodes } from "./convert";
import { cleanIndent, indentAfterBackspace, indentAfterShiftTab, indentAfterTab, listLevelAfter } from "./indentation";
import type { Comment, DocumentMeta, Indent, ListRef, Note, ParagraphData, ParagraphProps, Section, StyleSheet } from "./model";
import { isBulletList, listFor } from "./numbering";
import { schema } from "./schema";
import { findStyle, headingStyle, namedStyle, resolvedParagraphProps, styleHeadingLevel } from "./styles";

type Data = Record<string, unknown>;

export const metaOf = (state: EditorState | Transaction): DocumentMeta => (state.doc.attrs.meta ?? {}) as DocumentMeta;

/** sets the document's meta (styles, sections, notes...) as part of the transaction, so undo covers it. */
export function setMeta(tr: Transaction, meta: DocumentMeta): Transaction {
  return tr.setDocAttribute("meta", meta);
}

/** the paragraphs and headings the selection touches, with their positions. */
export function selectedTextblocks(state: EditorState): { node: PmNode; pos: number }[] {
  const blocks: { node: PmNode; pos: number }[] = [];
  const { from, to } = state.selection;
  state.doc.nodesBetween(from, to, (node, pos) => {
    if (node.isTextblock) {
      blocks.push({ node, pos });
      return false;
    }
    return true;
  });
  if (blocks.length === 0) {
    const $from = state.selection.$from;
    for (let depth = $from.depth; depth > 0; depth -= 1) {
      if ($from.node(depth).isTextblock) {
        blocks.push({ node: $from.node(depth), pos: $from.before(depth) });
        break;
      }
    }
  }
  return blocks;
}

const paragraphData = (node: PmNode) => node.attrs.data as ParagraphData;

/** a copy of a paragraph's data with its props changed; empty props are dropped. */
export function withProps(data: ParagraphData, change: (props: ParagraphProps) => ParagraphProps): ParagraphData {
  const props = change({ ...(data.props ?? {}) });
  const clean = Object.fromEntries(Object.entries(props).filter(([, value]) => value !== undefined)) as ParagraphProps;
  const next: ParagraphData = { ...data };
  if (Object.keys(clean).length) next.props = clean;
  else delete next.props;
  return next;
}

function setData(tr: Transaction, pos: number, node: PmNode, data: ParagraphData): void {
  tr.setNodeMarkup(pos, undefined, { ...node.attrs, data });
}

/** changes the props of every selected paragraph in one step. */
export function updateParagraphs(state: EditorState, change: (props: ParagraphProps, node: PmNode) => ParagraphProps): Transaction | null {
  const blocks = selectedTextblocks(state);
  if (blocks.length === 0) return null;
  const tr = state.tr;
  for (const { node, pos } of blocks) setData(tr, pos, node, withProps(paragraphData(node), (props) => change(props, node)));
  return tr;
}

// --- styles

export type StyleKey = string;

/** applies a style menu entry ("h1", "body", "title", "quote", "style:<id>") to the selected paragraphs. */
export function applyStyle(state: EditorState, key: StyleKey): Transaction | null {
  const blocks = selectedTextblocks(state);
  if (blocks.length === 0) return null;
  const meta = metaOf(state);
  let sheet: StyleSheet = meta.styles;
  let styleId: string | undefined;
  let level: number | null = null;
  if (/^h[1-6]$/.test(key)) {
    level = Number(key.slice(1));
    ({ id: styleId, sheet } = headingStyle(sheet, level));
  } else if (key === "title" || key === "quote") {
    ({ id: styleId, sheet } = namedStyle(sheet, key));
  } else if (key.startsWith("style:")) {
    styleId = key.slice(6);
    level = styleHeadingLevel(sheet, styleId);
  }
  const tr = state.tr;
  if (sheet !== meta.styles) setMeta(tr, { ...meta, styles: sheet });
  for (const { node, pos } of blocks) {
    const data: ParagraphData = withProps(paragraphData(node), (props) => ({ ...props, outlineLevel: undefined }));
    if (styleId === undefined) delete data.styleId;
    else data.styleId = styleId;
    const type = level === null ? schema.nodes.paragraph : schema.nodes.heading;
    tr.setNodeMarkup(pos, type, level === null ? { data } : { level, data });
  }
  return tr;
}

// --- marks

/** whether the whole selection (or the cursor's stored marks) has a mark type. */
export function markActive(state: EditorState, type: MarkType): boolean {
  const { from, to, empty, $from } = state.selection;
  if (empty) return !!type.isInSet(state.storedMarks ?? $from.marks());
  return state.doc.rangeHasMark(from, to, type);
}

/** the data of the first mark of `type` in the selection, or null. */
export function markData(state: EditorState, type: MarkType): Data | null {
  const { from, to, empty, $from } = state.selection;
  if (empty) return ((type.isInSet(state.storedMarks ?? $from.marks())?.attrs.data as Data) ?? null);
  let found: Data | null = null;
  state.doc.nodesBetween(from, to, (node) => {
    if (found || !node.isInline) return !found;
    const mark = type.isInSet(node.marks);
    if (mark) found = mark.attrs.data as Data;
    return false;
  });
  return found;
}

/** bold on or off for the selection, like the B button. */
export function toggleMark(state: EditorState, name: string, data: Data = {}): Transaction | null {
  const type = schema.marks[name];
  const { from, to, empty } = state.selection;
  const tr = state.tr;
  if (markActive(state, type)) {
    if (empty) tr.removeStoredMark(type);
    else tr.removeMark(from, to, type);
  } else if (empty) tr.addStoredMark(type.create({ data }));
  else tr.addMark(from, to, type.create({ data }));
  return tr;
}

/** sets a valued mark (font, size, colour, highlight), or removes it with null. */
export function setMark(state: EditorState, name: string, data: Data | null): Transaction {
  const type = schema.marks[name];
  const { from, to, empty } = state.selection;
  const tr = state.tr;
  if (empty) {
    tr.removeStoredMark(type);
    if (data) tr.addStoredMark(type.create({ data }));
    return tr;
  }
  tr.removeMark(from, to, type);
  if (data) tr.addMark(from, to, type.create({ data }));
  return tr;
}

// --- paragraph formatting

export function setAlignment(state: EditorState, alignment: ParagraphProps["alignment"]): Transaction | null {
  return updateParagraphs(state, (props) => ({ ...props, alignment }));
}

/** line spacing in lines (1, 1.15, 1.5, 2), as word's "auto" rule. */
export function setLineSpacing(state: EditorState, lines: number): Transaction | null {
  return updateParagraphs(state, (props) => ({ ...props, spacing: { ...props.spacing, line: { rule: "auto", value: Math.round(lines * 240) } } }));
}

export function setSpacing(state: EditorState, side: "before" | "after", twips: number): Transaction | null {
  return updateParagraphs(state, (props) => ({ ...props, spacing: { ...props.spacing, [side]: Math.max(0, Math.round(twips)) } }));
}

export function setIndent(state: EditorState, change: (indent: Indent) => Indent): Transaction | null {
  return updateParagraphs(state, (props) => ({ ...props, indent: cleanIndent(change({ ...(props.indent ?? {}) })) }));
}

/** the formatting that really applies to the paragraph with the cursor. */
export function currentParagraphProps(state: EditorState): { props: ParagraphProps; data: ParagraphData; node: PmNode } | null {
  const [first] = selectedTextblocks(state);
  if (!first) return null;
  const data = paragraphData(first.node);
  return { props: resolvedParagraphProps(metaOf(state), data.styleId, data.props), data, node: first.node };
}

// --- lists

/** the list the paragraph is in, by its own props or its style. */
export function listOf(meta: DocumentMeta, node: PmNode): ListRef | null {
  const data = paragraphData(node);
  const list = resolvedParagraphProps(meta, data.styleId, data.props).list;
  return list && list.listId !== "0" ? list : null;
}

/** turns bullets or numbers on for the selection, or off when they are already on. */
export function toggleList(state: EditorState, kind: "bullet" | "number"): Transaction | null {
  const blocks = selectedTextblocks(state);
  if (blocks.length === 0) return null;
  const meta = metaOf(state);
  const lists = blocks.map(({ node }) => listOf(meta, node));
  const allInKind = lists.every((list) => list && isBulletList(meta.numbering, list.listId) === (kind === "bullet"));
  const tr = state.tr;
  if (allInKind) {
    for (const { node, pos } of blocks) {
      const styleList = resolvedParagraphProps(meta, paragraphData(node).styleId, undefined).list;
      // a style's list is switched off with list id "0", as word does
      setData(tr, pos, node, withProps(paragraphData(node), (props) => ({ ...props, list: styleList ? { listId: "0", level: 0 } : undefined })));
    }
    return tr;
  }
  const before = previousListId(state, blocks[0].pos);
  const { listId, numbering } = listFor(meta.numbering, kind, before);
  if (numbering !== meta.numbering) setMeta(tr, { ...meta, numbering });
  for (const [index, { node, pos }] of blocks.entries()) {
    const level = lists[index]?.level ?? 0;
    setData(tr, pos, node, withProps(paragraphData(node), (props) => ({ ...props, list: { listId, level } })));
  }
  return tr;
}

/** the list of the paragraph just before `pos`, so a new item carries on its numbering. */
function previousListId(state: EditorState, pos: number): string | null {
  const $pos = state.doc.resolve(pos);
  const index = $pos.index($pos.depth);
  if (index === 0) return null;
  const previous = $pos.node($pos.depth).child(index - 1);
  return previous.isTextblock ? (listOf(metaOf(state), previous)?.listId ?? null) : null;
}

export function changeListLevel(state: EditorState, direction: "in" | "out"): Transaction | null {
  const meta = metaOf(state);
  const blocks = selectedTextblocks(state).filter(({ node }) => listOf(meta, node));
  if (blocks.length === 0) return null;
  const tr = state.tr;
  let changed = false;
  for (const { node, pos } of blocks) {
    const next = listLevelAfter(listOf(meta, node) as ListRef, direction);
    if (!next) continue;
    changed = true;
    setData(tr, pos, node, withProps(paragraphData(node), (props) => ({ ...props, list: next })));
  }
  return changed ? tr : null;
}

// --- smart indentation (Tab, Shift Tab, Backspace)

function atParagraphStart(state: EditorState): boolean {
  const { $from, empty } = state.selection;
  return empty && $from.parent.isTextblock && $from.parentOffset === 0;
}

/** Tab: list level in a list, first-line indent at a paragraph's start, otherwise null (a tab is typed). */
export function tabCommand(state: EditorState): Transaction | null {
  const multiple = selectedTextblocks(state).length > 1;
  const inList = selectedTextblocks(state).some(({ node }) => listOf(metaOf(state), node));
  if (inList && (multiple || atParagraphStart(state))) return changeListLevel(state, "in");
  if (multiple) return setIndent(state, (indent) => ({ ...indent, left: (indent.left ?? 0) + 720 }));
  if (!atParagraphStart(state)) return null;
  return setIndent(state, indentAfterTab);
}

export function shiftTabCommand(state: EditorState): Transaction | null {
  const inList = selectedTextblocks(state).some(({ node }) => listOf(metaOf(state), node));
  if (inList) return changeListLevel(state, "out");
  const current = currentParagraphProps(state);
  if (!current) return null;
  const next = indentAfterShiftTab(current.data.props?.indent ?? {});
  return next ? setIndent(state, () => next) : null;
}

/** Backspace at the very start of a paragraph with a first-line indent removes the indent first. */
export function backspaceIndent(state: EditorState): Transaction | null {
  if (!atParagraphStart(state)) return null;
  const current = currentParagraphProps(state);
  const next = current && indentAfterBackspace(current.data.props?.indent ?? {});
  return next ? setIndent(state, () => next) : null;
}

// --- inserting

export function insertTable(state: EditorState, rows: number, columns: number): Transaction {
  const meta = metaOf(state);
  const section = meta.sections?.[0];
  const textWidth = section ? section.page.width - section.page.margins.left - section.page.margins.right : 9360;
  const width = Math.floor(textWidth / columns);
  const border = { style: "single", size: 4 };
  const cell = () => schema.node("table_cell", { data: { width } }, [schema.node("paragraph", { data: {} })]);
  const table = schema.node(
    "table",
    { data: { columnWidths: Array(columns).fill(width), borders: { top: border, left: border, bottom: border, right: border, insideHorizontal: border, insideVertical: border } } },
    Array.from({ length: rows }, () => schema.node("table_row", { data: { header: false, cantSplit: false } }, Array.from({ length: columns }, cell))),
  );
  const tr = state.tr.replaceSelectionWith(table);
  // put the cursor in the first cell (table, row, cell, paragraph: four steps in)
  let start = -1;
  tr.doc.descendants((node, pos) => {
    if (node === table) start = pos;
    return start === -1;
  });
  return start >= 0 ? tr.setSelection(TextSelection.near(tr.doc.resolve(start + 4))) : tr;
}

export function insertImage(state: EditorState, mediaId: string, widthTwips: number, heightTwips: number, name: string): Transaction {
  const image = schema.node("image", { data: { mediaId, width: widthTwips, height: heightTwips, name, placement: { type: "inline" } } });
  return state.tr.replaceSelectionWith(image);
}

export function insertPageBreak(state: EditorState): Transaction {
  const tr = state.tr.deleteSelection();
  const $from = tr.selection.$from;
  // split the paragraph at the cursor and put the break between the halves
  if ($from.parent.isTextblock && $from.depth === 1) {
    const pos = $from.pos;
    tr.split(pos);
    // the split adds a closing and an opening token: between them is pos + 1
    tr.insert(pos + 1, schema.node("page_break", { data: {} }));
    return tr.setSelection(TextSelection.create(tr.doc, pos + 3));
  }
  return tr.replaceSelectionWith(schema.node("page_break", { data: {} }));
}

export function insertPageNumber(state: EditorState): Transaction {
  return state.tr.replaceSelectionWith(schema.node("field", { data: { kind: "pageNumber", instruction: "PAGE", result: "1" } }), true);
}

function nextId(ids: string[], prefix: string): string {
  const numbers = ids.map((id) => Number.parseInt(id.replace(/^\D+/, ""), 10)).filter(Number.isFinite);
  return `${prefix}${(numbers.length ? Math.max(...numbers) : 0) + 1}`;
}

/** a footnote at the cursor: a reference in the text and an empty note to type in. */
export function insertFootnote(state: EditorState, text = ""): { tr: Transaction; noteId: string } {
  const meta = metaOf(state);
  const notes = meta.notes ?? [];
  const noteId = nextId(notes.filter((note) => !note.special).map((note) => note.id), "fn");
  const sheet = meta.styles;
  const note: Note = {
    id: noteId,
    kind: "footnote",
    blocks: [{ type: "paragraph", ...(findStyle(sheet, "FootnoteText") ? { styleId: "FootnoteText" } : {}), content: text ? [{ type: "text", text }] : [] }],
  };
  const marks: PmMark[] = findStyle(sheet, "FootnoteReference")
    ? [schema.marks.characterStyle.create({ data: { styleId: "FootnoteReference" } })]
    : [schema.marks.superscript.create({ data: {} })];
  const tr = setMeta(state.tr, { ...meta, notes: [...notes, note] });
  tr.replaceSelectionWith(schema.node("note_ref", { data: { noteId } }, undefined, marks), false);
  return { tr, noteId };
}

/** a link on the selected text, or a new linked text at the cursor. */
export function setLink(state: EditorState, href: string | null, textIfEmpty?: string): Transaction {
  const type = schema.marks.link;
  const { from, to, empty } = state.selection;
  const tr = state.tr;
  if (!href) return tr.removeMark(from, to, type);
  const mark = type.create({ data: { href } });
  if (empty) {
    const text = textIfEmpty || href;
    tr.insert(from, schema.text(text, [mark, ...state.selection.$from.marks().filter((m) => m.type !== type)]));
    return tr;
  }
  return tr.removeMark(from, to, type).addMark(from, to, mark);
}

/** a comment on the selection: the comment mark over the text and its marker after it. */
export function addComment(state: EditorState, author: string, text: string, date: string): { tr: Transaction; commentId: string } | null {
  const { from, to, empty } = state.selection;
  if (empty) return null;
  const meta = metaOf(state);
  const comments = meta.comments ?? [];
  const commentId = nextId(comments.map((comment) => comment.id), "");
  const initials = author.split(/\s+/).map((part) => part[0] ?? "").join("").slice(0, 3).toUpperCase();
  const comment: Comment = { id: commentId, author, initials, date, blocks: [{ type: "paragraph", ...(findStyle(meta.styles, "CommentText") ? { styleId: "CommentText" } : {}), content: [{ type: "text", text }] }] };
  const tr = setMeta(state.tr, { ...meta, comments: [...comments, comment] });
  tr.addMark(from, to, schema.marks.comment.create({ data: { id: commentId } }));
  tr.insert(to, schema.node("comment_ref", { data: { commentId } }));
  return { tr, commentId };
}

/** removes a comment: its marks, its marker and its entry. */
export function deleteComment(state: EditorState, commentId: string): Transaction {
  const meta = metaOf(state);
  const tr = setMeta(state.tr, { ...meta, comments: (meta.comments ?? []).filter((comment) => comment.id !== commentId) });
  const refs: number[] = [];
  tr.doc.descendants((node, pos) => {
    if (node.type.name === "comment_ref" && (node.attrs.data as Data).commentId === commentId) refs.push(pos);
    for (const mark of node.marks) if (mark.type.name === "comment" && (mark.attrs.data as Data).id === commentId) tr.removeMark(pos, pos + node.nodeSize, mark);
    return true;
  });
  for (const pos of refs.reverse()) tr.delete(pos, pos + 1);
  return tr;
}

/** replaces a comment's words. */
export function editComment(state: EditorState, commentId: string, text: string): Transaction {
  const meta = metaOf(state);
  const comments = (meta.comments ?? []).map((comment) => {
    if (comment.id !== commentId) return comment;
    const first = comment.blocks[0] ?? { type: "paragraph", content: [] };
    return { ...comment, blocks: [{ ...first, content: [{ type: "text", text }] }] };
  });
  return setMeta(state.tr, { ...meta, comments });
}

/** a note's words, replaced by what the footnote area's editor holds. */
export function setNoteBlocks(state: EditorState, noteId: string, blocks: Note["blocks"]): Transaction {
  const meta = metaOf(state);
  return setMeta(state.tr, { ...meta, notes: (meta.notes ?? []).map((note) => (note.id === noteId ? { ...note, blocks } : note)) });
}

export function setHeaderFooterBlocks(state: EditorState, id: string, blocks: Note["blocks"]): Transaction {
  const meta = metaOf(state);
  return setMeta(state.tr, { ...meta, headersFooters: (meta.headersFooters ?? []).map((part) => (part.id === id ? { ...part, blocks } : part)) });
}

/** gives the first section a header or footer to type in, when it has none. */
export function addHeaderFooter(state: EditorState, kind: "header" | "footer"): { tr: Transaction; id: string } {
  const meta = metaOf(state);
  const parts = meta.headersFooters ?? [];
  const id = nextId(parts.map((part) => part.id), `letters-${kind}-`);
  const sections = (meta.sections ?? []).map((section) => {
    const refs = kind === "header" ? section.headers : section.footers;
    return { ...section, [kind === "header" ? "headers" : "footers"]: { ...refs, default: refs?.default ?? id } } as Section;
  });
  const tr = setMeta(state.tr, { ...meta, sections, headersFooters: [...parts, { id, kind, blocks: [{ type: "paragraph", content: [] }] }] });
  return { tr, id };
}

// --- the page (every section: a student paper has one)

export function updateSections(state: EditorState, change: (section: Section) => Section): Transaction {
  const meta = metaOf(state);
  return setMeta(state.tr, { ...meta, sections: (meta.sections ?? []).map(change) });
}

export const PAGE_SIZES = {
  letter: { label: "US Letter", width: 12240, height: 15840 },
  a4: { label: "A4", width: 11906, height: 16838 },
  legal: { label: "US Legal", width: 12240, height: 20160 },
} as const;

export function setPageSize(state: EditorState, size: keyof typeof PAGE_SIZES): Transaction {
  const { width, height } = PAGE_SIZES[size];
  return updateSections(state, (section) => {
    const landscape = section.page.orientation === "landscape";
    return { ...section, page: { ...section.page, width: landscape ? height : width, height: landscape ? width : height } };
  });
}

export function setOrientation(state: EditorState, orientation: "portrait" | "landscape"): Transaction {
  return updateSections(state, (section) => {
    const { width, height } = section.page;
    const wide = width > height;
    const swap = (orientation === "landscape") !== wide;
    return { ...section, page: { ...section.page, orientation, width: swap ? height : width, height: swap ? width : height } };
  });
}

export function setMargin(state: EditorState, side: "top" | "bottom" | "left" | "right", twips: number): Transaction {
  return updateSections(state, (section) => ({ ...section, page: { ...section.page, margins: { ...section.page.margins, [side]: Math.max(0, Math.round(twips)) } } }));
}

export function setColumns(state: EditorState, count: number): Transaction {
  return updateSections(state, (section) => ({ ...section, columns: { space: 720, separator: false, ...section.columns, count } }));
}

export function setDifferentFirstPage(state: EditorState, on: boolean): Transaction {
  return updateSections(state, (section) => ({ ...section, differentFirstPage: on }));
}

/** replaces the whole document (a preset's result) as one undoable step. */
export function replaceDocument(state: EditorState, next: PmNode): Transaction {
  const tr = state.tr.replaceWith(0, state.doc.content.size, next.content);
  tr.setDocAttribute("meta", next.attrs.meta);
  tr.setDocAttribute("emptyBody", next.attrs.emptyBody);
  return tr.setSelection(TextSelection.atStart(tr.doc));
}

// --- pictures

export function selectedImage(state: EditorState): { node: PmNode; pos: number } | null {
  const selection = state.selection;
  if (selection instanceof NodeSelection && selection.node.type.name === "image") return { node: selection.node, pos: selection.from };
  return null;
}

export function updateImage(state: EditorState, change: (data: Data) => Data): Transaction | null {
  const image = selectedImage(state);
  if (!image) return null;
  const tr = state.tr.setNodeMarkup(image.pos, undefined, { data: change({ ...(image.node.attrs.data as Data) }) }, image.node.marks);
  return tr.setSelection(NodeSelection.create(tr.doc, image.pos));
}

/** the picture's wrap: in line with text, or floating with text around it. */
export function imageWrap(data: Data): string {
  const placement = data.placement as { type?: string; wrap?: string } | undefined;
  return placement?.type === "floating" ? (placement.wrap ?? "square") : "inline";
}

export function placementFor(wrap: string, current: Data): Data {
  if (wrap === "inline") return { type: "inline" };
  const placement = current as { type?: string; horizontal?: Data; vertical?: Data };
  return {
    type: "floating",
    horizontal: placement.type === "floating" && placement.horizontal ? placement.horizontal : { relativeTo: "column", offset: 0 },
    vertical: placement.type === "floating" && placement.vertical ? placement.vertical : { relativeTo: "paragraph", offset: 0 },
    wrap,
  };
}

export { inlinesToNodes };
