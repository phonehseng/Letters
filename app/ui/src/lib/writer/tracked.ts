// tracked changes: listing them, accepting and rejecting them, and, while
// Track Changes is on, turning the person's edits into tracked ones.
//
// a tracked change is a `change` mark on inline content (see schema.ts):
// kind "insertion" shows the text underlined in colour, "deletion" shows
// it struck through and keeps it in the document until accepted. this is
// how the office model stores them, so they save back as real revisions.
//
// what is tracked: typing, deleting and pasting text and inline items.
// what is not (said in CHANGELOG.md): formatting changes, and splitting or
// joining paragraphs, which the model has no tracked form for.

import { Fragment, Slice, type Mark as PmMark, type Node as PmNode, type Schema } from "prosemirror-model";
import { TextSelection, type EditorState, type Transaction } from "prosemirror-state";
import { ReplaceStep } from "prosemirror-transform";

export type ChangeKind = "insertion" | "deletion";

export interface ChangeData {
  kind: ChangeKind;
  id: string;
  author: string;
  date?: string;
}

export interface TrackedChange extends ChangeData {
  from: number;
  to: number;
  text: string;
}

function changeMarks(node: PmNode): PmMark[] {
  return node.marks.filter((mark) => mark.type.name === "change");
}

const dataOf = (mark: PmMark) => mark.attrs.data as ChangeData;

/** every tracked change, joining neighbouring pieces of the same change. */
export function listChanges(doc: PmNode): TrackedChange[] {
  const changes: TrackedChange[] = [];
  doc.descendants((node, pos) => {
    if (!node.isInline) return true;
    for (const mark of changeMarks(node)) {
      const data = dataOf(mark);
      const last = [...changes].reverse().find((change) => change.kind === data.kind && change.id === data.id && change.author === data.author);
      const text = node.isText ? (node.text ?? "") : node.type.name === "image" ? "[picture]" : "";
      if (last && last.to === pos) {
        last.to = pos + node.nodeSize;
        last.text += text;
      } else {
        changes.push({ ...data, from: pos, to: pos + node.nodeSize, text });
      }
    }
    return false;
  });
  return changes.sort((a, b) => a.from - b.from);
}

/** removes one change's mark from its range (keeps the content). */
function dropMark(tr: Transaction, change: TrackedChange): void {
  tr.doc.nodesBetween(change.from, change.to, (node, pos) => {
    if (!node.isInline) return true;
    for (const mark of changeMarks(node)) {
      const data = dataOf(mark);
      if (data.kind === change.kind && data.id === change.id && data.author === change.author) {
        const from = Math.max(pos, change.from);
        const to = Math.min(pos + node.nodeSize, change.to);
        if (node.isText) tr.removeMark(from, to, mark);
        else tr.setNodeMarkup(pos, undefined, undefined, mark.removeFromSet(node.marks));
      }
    }
    return false;
  });
}

/** accepting keeps an insertion (its mark goes) and removes a deletion's text. */
export function acceptChange(tr: Transaction, change: TrackedChange): void {
  if (change.kind === "insertion") dropMark(tr, change);
  else tr.delete(change.from, change.to);
}

/** rejecting removes an insertion's text and keeps a deletion's text. */
export function rejectChange(tr: Transaction, change: TrackedChange): void {
  if (change.kind === "insertion") tr.delete(change.from, change.to);
  else dropMark(tr, change);
}

/** accepts or rejects every change, last first so positions stay valid. */
export function resolveAll(tr: Transaction, accept: boolean): number {
  const changes = listChanges(tr.doc);
  for (const change of [...changes].reverse()) {
    const current = listChanges(tr.doc).find((each) => each.kind === change.kind && each.id === change.id && each.from === change.from);
    if (!current) continue;
    if (accept) acceptChange(tr, current);
    else rejectChange(tr, current);
  }
  return changes.length;
}

/** the next free change id: word's ids are numbers, unique in the document. */
export function nextChangeId(doc: PmNode): number {
  let highest = 0;
  for (const change of listChanges(doc)) {
    const value = Number.parseInt(change.id, 10);
    if (Number.isFinite(value)) highest = Math.max(highest, value);
  }
  return highest + 1000;
}

// --- recording edits as tracked changes

export interface Tracker {
  author: string;
  /** iso 8601, the time the edit is recorded */
  date: string;
  /** hands out a new change id */
  nextId: () => string;
}

/** the change mark to use next to `pos`: an adjacent one of the same kind and author is continued. */
function changeMarkAt(schema: Schema, doc: PmNode, pos: number, kind: ChangeKind, tracker: Tracker): PmMark {
  const $pos = doc.resolve(pos);
  for (const node of [$pos.nodeBefore, $pos.nodeAfter]) {
    const mark = node && changeMarks(node).find((each) => dataOf(each).kind === kind && dataOf(each).author === tracker.author);
    if (mark) return mark;
  }
  return schema.marks.change.create({ data: { kind, id: tracker.nextId(), author: tracker.author, date: tracker.date } });
}

/** the slice with every inline item in it marked as inserted. */
function markInserted(slice: Slice, mark: PmMark): Slice {
  const markFragment = (fragment: Fragment): Fragment => {
    const nodes: PmNode[] = [];
    fragment.forEach((node) => {
      if (node.isInline) {
        // the inline items whose model form has no marks (bookmarks, comment markers) take none
        const allowed = node.type.spec.marks !== "";
        // text someone deletes and types again is new text, not their deletion
        const marks = changeMarks(node).length ? node.marks.filter((each) => each.type.name !== "change") : node.marks;
        nodes.push(allowed ? node.mark(mark.addToSet(marks)) : node);
      } else {
        nodes.push(node.copy(markFragment(node.content)));
      }
    });
    return Fragment.from(nodes);
  };
  return new Slice(markFragment(slice.content), slice.openStart, slice.openEnd);
}

const isOwnInsertion = (node: PmNode, author: string) =>
  changeMarks(node).some((mark) => dataOf(mark).kind === "insertion" && dataOf(mark).author === author);
const isDeletion = (node: PmNode) => changeMarks(node).some((mark) => dataOf(mark).kind === "deletion");

/**
 * keeps the content of [from, to) but marks it deleted. the person's own
 * tracked insertions in the range are simply removed (deleting what you
 * just typed is not a change to anyone else's text). returns the end of
 * the range after that.
 */
function markDeleted(tr: Transaction, from: number, to: number, tracker: Tracker): number {
  const schema = tr.doc.type.schema;
  const own: [number, number][] = [];
  const mark = changeMarkAt(schema, tr.doc, from, "deletion", tracker);
  tr.doc.nodesBetween(from, to, (node, pos) => {
    if (!node.isInline) return true;
    const start = Math.max(pos, from);
    const end = Math.min(pos + node.nodeSize, to);
    if (isOwnInsertion(node, tracker.author)) own.push([start, end]);
    else if (!isDeletion(node) && node.type.spec.marks !== "") {
      if (node.isText) tr.addMark(start, end, mark);
      else tr.setNodeMarkup(pos, undefined, undefined, mark.addToSet(node.marks));
    }
    return false;
  });
  for (const [start, end] of own.reverse()) tr.delete(start, end);
  return own.reduce((end, [start, stop]) => end - (stop - start), to);
}

/**
 * turns a transaction into a tracked one: what it deletes is kept and
 * marked deleted, what it inserts is marked inserted. returns null when
 * the transaction should go through unchanged (it changes nothing, comes
 * from undo, or has steps other than text replacement, such as formatting).
 */
export function trackTransaction(state: EditorState, tr: Transaction, tracker: Tracker): Transaction | null {
  if (!tr.docChanged || tr.getMeta("history$") || tr.getMeta("untracked")) return null;
  if (!tr.steps.every((step) => step instanceof ReplaceStep)) return null;
  const tracked = state.tr;
  // maps a position in the original transaction's document (after the
  // steps so far) to the same place in the tracked document
  let place = (pos: number) => pos;
  let cursor: number | null = null;
  tr.steps.forEach((step, index) => {
    const { from, to, slice } = step as unknown as { from: number; to: number; slice: Slice };
    const start = tracked.mapping.maps.length;
    const previous = place;
    const a = previous(from);
    const b = previous(to);
    // the deleted content stays, marked; the insertion goes after it
    const end = to > from ? markDeleted(tracked, a, b, tracker) : b;
    const size = slice.size;
    if (size > 0) {
      const mark = changeMarkAt(tracked.doc.type.schema, tracked.doc, end, "insertion", tracker);
      tracked.replace(end, end, markInserted(slice, mark));
    }
    const here = tracked.mapping.slice(start);
    place = (pos: number) => {
      if (pos < from) return here.map(previous(pos), -1);
      if (pos <= from + size) return end + (pos - from);
      return here.map(previous(pos - size + (to - from)), 1);
    };
    // backspace: the cursor goes before the text it just marked deleted
    const last = index === tr.steps.length - 1;
    if (last && size === 0 && to > from && state.selection.empty && state.selection.head === to) cursor = a;
  });
  const head = cursor ?? place(tr.selection.head);
  tracked.setSelection(TextSelection.near(tracked.doc.resolve(Math.min(Math.max(head, 0), tracked.doc.content.size))));
  if (tr.storedMarks) tracked.setStoredMarks(tr.storedMarks);
  tracked.setMeta("tracked", true);
  return tracked;
}
