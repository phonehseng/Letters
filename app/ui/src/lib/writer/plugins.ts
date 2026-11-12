// the editor's behaviour: keys, typing rules, list labels and note
// numbers, find highlights, page boundaries, and the guard that keeps
// preserved content from being deleted by accident.

import { baseKeymap, chainCommands, splitBlockAs } from "prosemirror-commands";
import { dropCursor } from "prosemirror-dropcursor";
import { gapCursor } from "prosemirror-gapcursor";
import { history, redo, undo } from "prosemirror-history";
import { keymap } from "prosemirror-keymap";
import type { Node as PmNode } from "prosemirror-model";
import { NodeSelection, Plugin, PluginKey, TextSelection, type Command, type EditorState, type Transaction } from "prosemirror-state";
import { goToNextCell, tableEditing } from "prosemirror-tables";
import { Decoration, DecorationSet, type EditorView } from "prosemirror-view";
import { indentCss } from "./css";
import { backspaceIndent, listOf, metaOf, shiftTabCommand, tabCommand, toggleList } from "./editing";
import type { Match } from "./find";
import type { DocumentMeta, ParagraphData } from "./model";
import { ListCounter } from "./numbering";
import { schema } from "./schema";
import { findStyle, resolvedParagraphProps } from "./styles";
import { onType, type Replacement, type TypingSettings } from "./typing";

const PRESERVED = new Set(["preserved_block", "preserved_inline", "preserved_run"]);

const dispatchCommand =
  (make: (state: EditorState) => Transaction | null): Command =>
  (state, dispatch) => {
    const tr = make(state);
    if (!tr) return false;
    dispatch?.(tr.scrollIntoView());
    return true;
  };

// --- Enter: the next paragraph follows the style's "next", without the ids

function nextParagraph(node: PmNode, atEnd: boolean, meta: DocumentMeta): { type: typeof schema.nodes.paragraph; attrs: { data: ParagraphData } } | null {
  const data = node.attrs.data as ParagraphData;
  const styleId = data.styleId;
  const next = atEnd ? (findStyle(meta.styles, styleId)?.next ?? (node.type.name === "heading" ? undefined : styleId)) : styleId;
  const nextData: ParagraphData = {};
  if (next && !findStyle(meta.styles, next)?.isDefault) nextData.styleId = next;
  // the new paragraph keeps the formatting, not word's paragraph ids
  if (node.type.name !== "heading" && data.props) nextData.props = data.props;
  if (!atEnd && node.type.name === "heading") return null;
  return { type: schema.nodes.paragraph, attrs: { data: nextData } };
}

const enter: Command = (state, dispatch, view) => {
  const { $from, empty } = state.selection;
  // Enter in an empty list item ends the list, as in every word processor
  if (empty && $from.parent.isTextblock && $from.parent.content.size === 0 && listOf(metaOf(state), $from.parent)) {
    const tr = toggleList(state, "bullet") ?? null;
    if (tr && dispatch) dispatch(tr);
    return !!tr;
  }
  return splitBlockAs((node, atEnd) => nextParagraph(node, atEnd, metaOf(state)))(state, dispatch, view);
};

// --- Backspace and Delete next to preserved items select them first

function selectPreserved(direction: -1 | 1): Command {
  return (state, dispatch) => {
    const { $from, empty } = state.selection;
    if (!empty) return false;
    const inline = direction < 0 ? $from.nodeBefore : $from.nodeAfter;
    if (inline && PRESERVED.has(inline.type.name)) {
      const pos = direction < 0 ? $from.pos - inline.nodeSize : $from.pos;
      dispatch?.(state.tr.setSelection(NodeSelection.create(state.doc, pos)));
      return true;
    }
    // at the edge of a paragraph, the block next to it
    const atEdge = direction < 0 ? $from.parentOffset === 0 : $from.parentOffset === $from.parent.content.size;
    if (!atEdge || $from.depth < 1) return false;
    const blockPos = direction < 0 ? $from.before() : $from.after();
    const $block = state.doc.resolve(blockPos);
    const neighbour = direction < 0 ? $block.nodeBefore : $block.nodeAfter;
    if (neighbour && PRESERVED.has(neighbour.type.name)) {
      dispatch?.(state.tr.setSelection(NodeSelection.create(state.doc, direction < 0 ? blockPos - neighbour.nodeSize : blockPos)));
      return true;
    }
    return false;
  };
}

export interface WriterKeys {
  /** a writer command by name, for the few keys the editor handles itself */
  onTab?: () => void;
}

export function writerKeymap(): Plugin[] {
  return [
    keymap({
      "Mod-z": undo,
      "Mod-Shift-z": redo,
      "Mod-y": redo,
      Enter: enter,
      "Shift-Enter": (state, dispatch) => {
        dispatch?.(state.tr.replaceSelectionWith(schema.node("hard_break", { data: { kind: "line" } })).scrollIntoView());
        return true;
      },
      Tab: chainCommands(goToNextCell(1), dispatchCommand(tabCommand), (state, dispatch) => {
        dispatch?.(state.tr.replaceSelectionWith(schema.node("tab", { data: {} })).scrollIntoView());
        return true;
      }),
      "Shift-Tab": chainCommands(goToNextCell(-1), dispatchCommand(shiftTabCommand), () => true),
      Backspace: chainCommands(undoAutocorrect, selectPreserved(-1), dispatchCommand(backspaceIndent)),
      Delete: selectPreserved(1),
    }),
    keymap(baseKeymap),
  ];
}

// --- typing rules (smart quotes, autocorrect, capitals)

interface Autocorrected {
  from: number;
  to: number;
  /** what was there before the change, typed character included */
  original: string;
}

const typingKey = new PluginKey<Autocorrected | null>("typing");

/** the text of the paragraph before the cursor; atoms count as one object character. */
function textBefore(state: EditorState, pos: number): string {
  const $pos = state.doc.resolve(pos);
  return $pos.parent.textBetween(0, $pos.parentOffset, undefined, "ï¿¼");
}

/** utf-16 length of the last `count` characters of `text` (a character can be two units). */
function unitsOfLast(text: string, count: number): number {
  if (count <= 0) return 0;
  const chars = Array.from(text);
  return chars.slice(Math.max(0, chars.length - count)).join("").length;
}

export function typingRules(settings: () => TypingSettings, onChange: (change: Replacement) => void): Plugin {
  return new Plugin<Autocorrected | null>({
    key: typingKey,
    state: {
      init: () => null,
      apply: (tr, value) => {
        const meta = tr.getMeta(typingKey);
        if (meta !== undefined) return meta as Autocorrected | null;
        return tr.docChanged || tr.selectionSet ? null : value;
      },
    },
    props: {
      handleTextInput(view, from, to, text) {
        if (text.length !== 1 && Array.from(text).length !== 1) return false;
        if (view.composing) return false;
        const state = view.state;
        const before = textBefore(state, from);
        const change = onType(before, text, settings());
        if (!change) return false;
        const removed = unitsOfLast(before, change.deleteBefore);
        const start = from - removed;
        const marks = state.storedMarks ?? state.doc.resolve(from).marks();
        const original = state.doc.textBetween(start, from) + text;
        const tr = state.tr.replaceWith(start, to, schema.text(change.insert, marks));
        tr.setSelection(TextSelection.create(tr.doc, start + change.insert.length));
        // a plain capital or quote needs no undo offer: it is what was typed
        const offered = change.feature !== "smartQuotes" && change.feature !== "capitalizeSentences";
        tr.setMeta(typingKey, offered ? { from: start, to: start + change.insert.length, original } : null);
        view.dispatch(tr.scrollIntoView());
        onChange(change);
        return true;
      },
    },
  });
}

/** Backspace right after an autocorrection puts back what was typed. */
const undoAutocorrect: Command = (state, dispatch) => {
  const last = typingKey.getState(state);
  if (!last || !state.selection.empty || state.selection.from !== last.to) return false;
  if (dispatch) {
    const marks = state.doc.resolve(last.from).marks();
    const tr = state.tr.replaceWith(last.from, last.to, schema.text(last.original, marks));
    tr.setSelection(TextSelection.create(tr.doc, last.from + last.original.length)).setMeta(typingKey, null);
    dispatch(tr);
  }
  return true;
};

// --- list labels and note numbers (decorations, worked out from the document)

function labelDecorations(doc: PmNode): DecorationSet {
  const meta = metaOf({ doc } as EditorState);
  if (!meta.styles) return DecorationSet.empty;
  const counter = new ListCounter(meta.numbering ?? { definitions: [], instances: [] });
  const noteKinds = new Map((meta.notes ?? []).map((note) => [note.id, note.kind]));
  const numbers = { footnote: 0, endnote: 0 };
  const decorations: Decoration[] = [];
  doc.descendants((node, pos) => {
    if (node.isTextblock) {
      const data = node.attrs.data as ParagraphData;
      const props = resolvedParagraphProps(meta, data.styleId, data.props);
      const list = props.list && props.list.listId !== "0" ? props.list : null;
      const label = list ? counter.next(list.listId, list.level) : null;
      if (label !== null) {
        const hang = props.indent?.hanging ?? 0;
        decorations.push(
          Decoration.node(pos, pos + node.nodeSize, {
            class: "w-list-item",
            "data-label": label,
            style: [...indentCss(props), `--w-hang:${Math.max(hang / 15, 18)}px`].join(";"),
          }),
        );
      }
      return true;
    }
    if (node.type.name === "note_ref") {
      const kind = noteKinds.get(String((node.attrs.data as { noteId?: string }).noteId)) === "endnote" ? "endnote" : "footnote";
      numbers[kind] += 1;
      const text = kind === "endnote" ? toRoman(numbers.endnote) : String(numbers.footnote);
      decorations.push(Decoration.node(pos, pos + node.nodeSize, { "data-number": text }));
    }
    return true;
  });
  return DecorationSet.create(doc, decorations);
}

function toRoman(value: number): string {
  const table: [number, string][] = [[10, "x"], [9, "ix"], [5, "v"], [4, "iv"], [1, "i"]];
  let rest = value;
  let out = "";
  for (const [amount, text] of table) while (rest >= amount) ((out += text), (rest -= amount));
  return out;
}

/** true when a transaction only typed or deleted text inside one paragraph. */
function onlyTextChanged(tr: Transaction): boolean {
  return tr.steps.every((step) => {
    const json = step.toJSON() as { stepType: string; slice?: { content?: { type: string }[]; openStart?: number } };
    if (json.stepType !== "replace") return false;
    const content = json.slice?.content ?? [];
    return content.every((node) => node.type === "text") && !json.slice?.openStart;
  }) && tr.getMeta("labels") === undefined;
}

const labelsKey = new PluginKey<DecorationSet>("labels");

export function listLabels(): Plugin<DecorationSet> {
  return new Plugin<DecorationSet>({
    key: labelsKey,
    state: {
      init: (_, state) => labelDecorations(state.doc),
      apply: (tr, set) => {
        if (!tr.docChanged) return set;
        // typing inside a paragraph cannot change a label: just move them
        if (onlyTextChanged(tr)) return set.map(tr.mapping, tr.doc);
        return labelDecorations(tr.doc);
      },
    },
    props: { decorations: (state) => labelsKey.getState(state) },
  });
}

// --- find highlights

export const findKey = new PluginKey<{ matches: Match[]; current: number }>("find");

export function findHighlights(): Plugin {
  return new Plugin({
    key: findKey,
    state: {
      init: () => ({ matches: [] as Match[], current: -1 }),
      apply: (tr, value) => {
        const meta = tr.getMeta(findKey);
        if (meta) return meta;
        if (!tr.docChanged) return value;
        return { matches: value.matches.map((match) => ({ from: tr.mapping.map(match.from), to: tr.mapping.map(match.to) })), current: value.current };
      },
    },
    props: {
      decorations(state) {
        const { matches, current } = findKey.getState(state)!;
        if (matches.length === 0) return DecorationSet.empty;
        return DecorationSet.create(
          state.doc,
          matches.map((match, index) => Decoration.inline(match.from, match.to, { class: index === current ? "w-find w-find-current" : "w-find" })),
        );
      },
    },
  });
}

// --- page boundaries (placed by the surface after it measures, see pagination.ts)

export interface PageGap {
  /** the top-level block the new page starts with */
  blockIndex: number;
  /** css pixels at 100%: the rest of the page above, its bottom margin, the desk and the next top margin */
  height: number;
  /** the number of the page that starts here */
  pageNumber: number;
  render: () => HTMLElement;
}

export const pagesKey = new PluginKey<PageGap[]>("pages");

export function pageBoundaries(): Plugin {
  return new Plugin({
    key: pagesKey,
    state: {
      init: () => [] as PageGap[],
      apply: (tr, gaps) => (tr.getMeta(pagesKey) as PageGap[] | undefined) ?? gaps,
    },
    props: {
      decorations(state) {
        const gaps = pagesKey.getState(state) ?? [];
        if (gaps.length === 0) return DecorationSet.empty;
        const positions: number[] = [];
        let pos = 0;
        state.doc.forEach((node) => {
          positions.push(pos);
          pos += node.nodeSize;
        });
        return DecorationSet.create(
          state.doc,
          gaps
            .filter((gap) => gap.blockIndex < positions.length)
            .map((gap) =>
              Decoration.widget(positions[gap.blockIndex], gap.render, { side: -1, key: `page-${gap.pageNumber}-${Math.round(gap.height)}`, ignoreSelection: true }),
            ),
        );
      },
    },
  });
}

// --- the guard for preserved items

function countPreserved(doc: PmNode): number {
  let count = 0;
  doc.descendants((node) => {
    if (PRESERVED.has(node.type.name)) count += 1;
    return !node.isTextblock || node.childCount > 0;
  });
  return count;
}

function stepRemovesPreserved(tr: Transaction): boolean {
  return tr.steps.some((step, index) => {
    const { from, to } = step as unknown as { from?: number; to?: number };
    if (from === undefined || to === undefined || to <= from) return false;
    let found = false;
    tr.docs[index].nodesBetween(from, to, (node) => {
      if (PRESERVED.has(node.type.name)) found = true;
      return !found;
    });
    return found;
  });
}

/**
 * preserved items (a chart, a text box...) can't be deleted by accident:
 * deleting a range of text that holds one is refused, and Backspace next
 * to one selects it first. deleting it on purpose (selected on its own,
 * then Delete) works.
 */
export function preservedGuard(onRefused: () => void): Plugin {
  return new Plugin({
    filterTransaction(tr, state) {
      if (!tr.docChanged || tr.getMeta("history$") || tr.getMeta("allowPreservedDelete")) return true;
      if (!stepRemovesPreserved(tr)) return true;
      const before = countPreserved(state.doc);
      const after = countPreserved(tr.doc);
      if (after >= before) return true;
      const selection = state.selection;
      const onPurpose = selection instanceof NodeSelection && PRESERVED.has(selection.node.type.name) && before - after === 1;
      if (!onPurpose) setTimeout(onRefused, 0);
      return onPurpose;
    },
  });
}

// --- word's paragraph ids must stay unique (a split or paste would copy them)

export function uniqueParagraphIds(): Plugin {
  return new Plugin({
    appendTransaction(transactions, _old, state) {
      if (!transactions.some((tr) => tr.docChanged && !onlyTextChanged(tr))) return null;
      const seen = new Set<string>();
      const duplicates: { pos: number; node: PmNode }[] = [];
      state.doc.descendants((node, pos) => {
        if (!node.isTextblock) return true;
        const attributes = (node.attrs.data as ParagraphData).attributes as { name: string; value: string }[] | undefined;
        const id = attributes?.find((attribute) => attribute.name === "w14:paraId")?.value;
        if (id) {
          if (seen.has(id)) duplicates.push({ pos, node });
          seen.add(id);
        }
        return false;
      });
      if (duplicates.length === 0) return null;
      const tr = state.tr;
      for (const { pos, node } of duplicates) {
        const data = { ...(node.attrs.data as ParagraphData) };
        delete data.attributes;
        tr.setNodeMarkup(pos, undefined, { ...node.attrs, data });
      }
      return tr.setMeta("addToHistory", false);
    },
  });
}

/** every plugin of the main editor and of the small ones (headers, notes). */
export function writerPlugins(options: { typing: () => TypingSettings; onAutocorrect: (change: Replacement) => void; onPreservedRefused: () => void }): Plugin[] {
  return [
    typingRules(options.typing, options.onAutocorrect),
    ...writerKeymap(),
    history(),
    tableEditing(),
    gapCursor(),
    dropCursor({ color: "var(--accent)", width: 2 }),
    listLabels(),
    findHighlights(),
    pageBoundaries(),
    preservedGuard(options.onPreservedRefused),
    uniqueParagraphIds(),
  ];
}

export type { EditorView };
