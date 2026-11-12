// tracked changes: listing, accepting, rejecting, and recording edits, formatting, splits and joins
import { describe, expect, it } from "vitest";
import { TextSelection, type EditorState } from "prosemirror-state";
import { docToModel } from "./convert";
import { acceptChange, listChanges, nextChangeId, rejectChange, resolveAll, trackTransaction, type Tracker } from "./tracked";
import { para, select, startOf, stateWith } from "./testDocs";

const changed = () =>
  stateWith([
    {
      type: "paragraph",
      content: [
        { type: "text", text: "Kept " },
        { type: "text", text: "added", marks: [{ type: "insertion", id: "1", author: "Ada", date: "2026-11-11T09:00:00Z" }] },
        { type: "text", text: " gone", marks: [{ type: "deletion", id: "2", author: "Ada" }] },
      ],
    },
  ]);

describe("tracked changes", () => {
  it("lists each change with its text and author", () => {
    const changes = listChanges(changed().doc);
    expect(changes.map((change) => [change.kind, change.text, change.author])).toEqual([
      ["insertion", "added", "Ada"],
      ["deletion", " gone", "Ada"],
    ]);
    expect(nextChangeId(changed().doc)).toBeGreaterThan(2);
  });

  it("accepting keeps an insertion and removes a deletion", () => {
    const state = changed();
    const tr = state.tr;
    const [insertion, deletion] = listChanges(state.doc);
    acceptChange(tr, deletion);
    acceptChange(tr, insertion);
    expect(tr.doc.textContent).toBe("Kept added");
    expect(listChanges(tr.doc)).toEqual([]);
    expect(docToModel(tr.doc).blocks[0].content).toEqual([{ type: "text", text: "Kept added" }]);
  });

  it("rejecting removes an insertion and keeps a deletion", () => {
    const state = changed();
    const tr = state.tr;
    const [insertion, deletion] = listChanges(state.doc);
    rejectChange(tr, deletion);
    rejectChange(tr, insertion);
    expect(tr.doc.textContent).toBe("Kept  gone");
    expect(listChanges(tr.doc)).toEqual([]);
  });

  it("accept all and reject all", () => {
    const accepted = changed().tr;
    expect(resolveAll(accepted, true)).toBe(2);
    expect(accepted.doc.textContent).toBe("Kept added");
    const rejected = changed().tr;
    resolveAll(rejected, false);
    expect(rejected.doc.textContent).toBe("Kept  gone");
  });
});

describe("recording edits while Track Changes is on", () => {
  let id = 100;
  const tracker: Tracker = { author: "Ben", date: "2026-11-12T10:00:00Z", nextId: () => String((id += 1)) };
  const apply = (state: EditorState, tr = state.tr) => state.apply(trackTransaction(state, tr, tracker) ?? tr);

  it("typed text is marked inserted, and typing on continues the same change", () => {
    let state = select(stateWith([para("Hello")]), startOf(stateWith([para("Hello")]), 0) + 5);
    state = apply(state, state.tr.insertText("!"));
    state = apply(state, state.tr.insertText("?"));
    const changes = listChanges(state.doc);
    expect(changes).toHaveLength(1);
    expect(changes[0]).toMatchObject({ kind: "insertion", author: "Ben", text: "!?" });
    expect(state.selection.from).toBe(startOf(state, 0) + 7);
  });

  it("deleted text stays, marked deleted, and backspace moves before it", () => {
    let state = stateWith([para("Hello")]);
    const end = startOf(state, 0) + 5;
    state = select(state, end);
    state = apply(state, state.tr.delete(end - 1, end));
    expect(state.doc.textContent).toBe("Hello");
    expect(listChanges(state.doc)[0]).toMatchObject({ kind: "deletion", text: "o" });
    expect(state.selection.from).toBe(end - 1);
  });

  it("deleting your own tracked insertion really removes it", () => {
    let state = select(stateWith([para("Hi")]), startOf(stateWith([para("Hi")]), 0) + 2);
    state = apply(state, state.tr.insertText("!"));
    const at = state.selection.from;
    state = apply(state, state.tr.delete(at - 1, at));
    expect(state.doc.textContent).toBe("Hi");
    expect(listChanges(state.doc)).toEqual([]);
  });

  it("typing over a selection deletes it and inserts after it", () => {
    let state = stateWith([para("one two")]);
    const start = startOf(state, 0);
    state = state.apply(state.tr.setSelection(TextSelection.create(state.doc, start + 4, start + 7)));
    state = apply(state, state.tr.insertText("three"));
    expect(state.doc.textContent).toBe("one twothree");
    expect(listChanges(state.doc).map((change) => [change.kind, change.text])).toEqual([
      ["deletion", "two"],
      ["insertion", "three"],
    ]);
  });

  it("undo goes through untouched", () => {
    const state = stateWith([para("Bold me")]);
    expect(trackTransaction(state, state.tr.insertText("x").setMeta("history$", {}), tracker)).toBeNull();
  });

  it("formatting is recorded as Word's w:rPrChange, with the old formatting kept", () => {
    let state = stateWith([{ type: "paragraph", content: [{ type: "text", text: "Bold me", marks: [{ type: "italic" }] }] }]);
    const start = startOf(state, 0);
    const bold = state.tr.addMark(start, start + 4, state.schema.marks.bold.create({ data: {} }));
    state = apply(state, bold);
    const [change] = listChanges(state.doc);
    expect(change).toMatchObject({ kind: "formatting", author: "Ben", text: "Bold" });
    const content = docToModel(state.doc).blocks[0].content as { text: string; marks: { type: string; xml?: string }[] }[];
    expect(content[0].marks.map((mark) => mark.type)).toEqual(["bold", "italic", "preserved"]);
    expect(content[0].marks[2].xml).toBe('<w:rPrChange w:id="' + change.id + '" w:author="Ben" w:date="2026-11-12T10:00:00Z"><w:rPr><w:i/></w:rPr></w:rPrChange>');
    // a second change to the same text keeps the first record (Word keeps the original formatting)
    const again = apply(state, state.tr.addMark(start, start + 4, state.schema.marks.underline.create({ data: { style: "single" } })));
    expect(listChanges(again.doc).filter((each) => each.kind === "formatting")).toHaveLength(1);
  });

  it("accepting a formatting change keeps it; rejecting puts the old formatting back", () => {
    let state = stateWith([para("Plain")]);
    const start = startOf(state, 0);
    state = apply(state, state.tr.addMark(start, start + 5, state.schema.marks.bold.create({ data: {} })));
    const [change] = listChanges(state.doc);
    const accepted = state.tr;
    acceptChange(accepted, change);
    expect(docToModel(accepted.doc).blocks[0].content).toEqual([{ type: "text", text: "Plain", marks: [{ type: "bold" }] }]);
    const rejected = state.tr;
    rejectChange(rejected, change);
    expect(docToModel(rejected.doc).blocks[0].content).toEqual([{ type: "text", text: "Plain" }]);
  });

  it("splitting a paragraph records its new paragraph mark; rejecting joins it back", () => {
    let state = stateWith([para("One two")]);
    const at = startOf(state, 0) + 3;
    state = select(state, at);
    state = apply(state, state.tr.split(at));
    expect(state.doc.childCount).toBe(2);
    const [change] = listChanges(state.doc);
    expect(change).toMatchObject({ kind: "paragraphInsertion", author: "Ben", text: "¶" });
    expect(docToModel(state.doc).blocks[0].props).toEqual({ preserved: [`<w:rPr><w:ins w:id="${change.id}" w:author="Ben" w:date="2026-11-12T10:00:00Z"/></w:rPr>`] });
    const rejected = state.tr;
    rejectChange(rejected, change);
    expect(rejected.doc.childCount).toBe(1);
    expect(rejected.doc.textContent).toBe("One two");
    expect(docToModel(rejected.doc).blocks[0].props).toBeUndefined();
    const accepted = state.tr;
    acceptChange(accepted, change);
    expect(accepted.doc.childCount).toBe(2);
    expect(listChanges(accepted.doc)).toEqual([]);
  });

  it("joining paragraphs keeps both and records the mark as deleted; accepting joins them", () => {
    let state = stateWith([para("One"), para("Two")]);
    const end = startOf(state, 0) + 3;
    const next = startOf(state, 1);
    state = select(state, next);
    state = apply(state, state.tr.delete(end, next));
    expect(state.doc.childCount).toBe(2);
    const [change] = listChanges(state.doc);
    expect(change.kind).toBe("paragraphDeletion");
    expect(state.selection.from).toBe(end);
    const accepted = state.tr;
    acceptChange(accepted, change);
    expect(accepted.doc.childCount).toBe(1);
    expect(accepted.doc.textContent).toBe("OneTwo");
    const rejected = state.tr;
    rejectChange(rejected, change);
    expect(rejected.doc.childCount).toBe(2);
    expect(listChanges(rejected.doc)).toEqual([]);
  });

  it("joining back a split you made yourself just joins", () => {
    let state = stateWith([para("One two")]);
    const at = startOf(state, 0) + 3;
    state = apply(select(state, at), select(state, at).tr.split(at));
    const end = startOf(state, 0) + 3;
    const next = startOf(state, 1);
    state = apply(select(state, next), select(state, next).tr.delete(end, next));
    expect(state.doc.childCount).toBe(1);
    expect(listChanges(state.doc)).toEqual([]);
  });
});
