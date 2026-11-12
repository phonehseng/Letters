// text blocks: the converter, the engine's offsets, and applying the engine's answer as one undo step
import { describe, expect, it } from "vitest";
import { EditorState, NodeSelection, TextSelection } from "prosemirror-state";
import { undo } from "prosemirror-history";
import { docToModel, modelToDoc } from "./convert";
import type { Block, DocumentModel } from "./model";
import { applyEngineModel, blockDiff, boundaryWithoutBox, cursorInTopParagraph, editorOffset, engineOffset, imageRef, nearestBoundary, selectedBlockRange, stableJson, textBoxAt } from "./textBlocks";
import { modelWith, para, select, startOf, stateWith } from "./testDocs";
import { writerPlugins } from "./plugins";

/** a text box block as the engine writes one (floating.rs TextBox::new). */
const textBox = (blocks: Block[], extra: Record<string, unknown> = {}): Block => ({
  type: "textBox",
  horizontal: { relativeTo: "page", offset: 1440 },
  vertical: { relativeTo: "page", offset: 1440 },
  width: 4536,
  height: 1440,
  autoHeight: true,
  wrap: "square",
  zOrder: 1,
  margins: { left: 144, top: 72, right: 144, bottom: 72 },
  border: { style: "single", size: 4, color: "000000" },
  fill: "FFFFFF",
  blocks,
  ...extra,
});

const roundTrip = (model: DocumentModel) => {
  const doc = modelToDoc(model);
  doc.check();
  return docToModel(doc);
};

describe("text blocks in the editor", () => {
  it("a text block round-trips exactly, its blocks editable inside it", () => {
    const model = modelWith([para("Before"), textBox([para("Inside", { styleId: "Quote" }), { type: "table", rows: [{ cells: [{ colSpan: 1, rowSpan: 1, blocks: [para("cell")] }] }] }]), para("After")]);
    expect(roundTrip(model)).toEqual(model);
    const doc = modelToDoc(model);
    expect(doc.child(1).type.name).toBe("text_box");
    expect(doc.child(1).child(0).type.name).toBe("paragraph");
  });

  it("an empty text block, rotation, name and exact sizes survive", () => {
    const model = modelWith([textBox([], { rotation: 15, name: "Box 1", widthEmu: 2880000, heightEmu: 914400 })]);
    expect(roundTrip(model)).toEqual(model);
  });

  it("finds the text block around the cursor", () => {
    const state = stateWith([para("a"), textBox([para("inside")])]);
    const inside = select(state, startOf(state, 1) + 1);
    expect(textBoxAt(inside)?.index).toBe(1);
    expect(textBoxAt(select(state, 1))).toBeNull();
    expect(cursorInTopParagraph(inside)).toBeNull();
  });
});

describe("engine offsets", () => {
  const state = stateWith([
    {
      type: "paragraph",
      content: [{ type: "bookmarkStart", id: "0", name: "x" }, { type: "text", text: "a😀b" }, { type: "tab" }, { type: "text", text: "cd" }, { type: "bookmarkEnd", id: "0" }],
    },
  ]);
  const paragraph = state.doc.child(0);

  it("count characters, not utf-16 units, and bookmarks as nothing", () => {
    // positions: bookmark 0-1, "a😀b" 1-5 (the emoji is two units), tab 5-6, "cd" 6-8
    expect(engineOffset(paragraph, 0)).toBe(0);
    expect(engineOffset(paragraph, 1)).toBe(0);
    expect(engineOffset(paragraph, 2)).toBe(1);
    expect(engineOffset(paragraph, 4)).toBe(2);
    expect(engineOffset(paragraph, 6)).toBe(4);
    expect(engineOffset(paragraph, 7)).toBe(5);
  });

  it("go back to the same editor offsets", () => {
    for (const offset of [2, 4, 5, 6, 7]) expect(editorOffset(paragraph, engineOffset(paragraph, offset))).toBe(offset);
    expect(editorOffset(paragraph, 99)).toBe(paragraph.content.size);
  });

  it("the cursor in a top-level paragraph, in the engine's terms", () => {
    const at = select(stateWith([para("x"), para("Hello world")]), startOf(stateWith([para("x"), para("Hello world")]), 1) + 6);
    expect(cursorInTopParagraph(at)).toEqual({ block: 1, offset: 6 });
  });
});

describe("applying the engine's answer", () => {
  it("only the changed blocks are replaced, in one undo step", () => {
    const blocks = [para("one"), para("Hello world"), para("three")];
    let state = EditorState.create({ doc: modelToDoc(modelWith(blocks)), plugins: writerPlugins({ typing: () => ({}) as never, onAutocorrect: () => {}, onPreservedRefused: () => {} }) });
    // what floating::split_paragraph_into_text_boxes gives for an offset of 6
    const answer = modelWith([para("one"), textBox([para("Hello ")]), textBox([para("world")], { horizontal: { relativeTo: "page", offset: 6264 }, zOrder: 2 }), para("three")]);
    const first = state.doc.child(0);
    const tr = applyEngineModel(state, answer)!;
    expect(tr.steps).toHaveLength(1);
    state = state.apply(tr);
    expect(docToModel(state.doc)).toEqual(answer);
    // the blocks around the change are the very same nodes
    expect(state.doc.child(0)).toBe(first);
    undo(state, (undone) => (state = state.apply(undone)));
    expect(docToModel(state.doc).blocks).toEqual(blocks);
  });

  it("nothing changed gives no transaction; a changed style sheet is set too", () => {
    const state = stateWith([para("x")]);
    expect(applyEngineModel(state, docToModel(state.doc))).toBeNull();
    const fixed = { ...docToModel(state.doc), styles: { styles: [{ id: "Normal", name: "Normal", kind: "paragraph" as const, isDefault: true, run: { font: { name: "Times New Roman" } } }] } };
    const tr = applyEngineModel(state, fixed)!;
    expect((tr.doc.attrs.meta as DocumentModel).styles.styles[0].run?.font?.name).toBe("Times New Roman");
  });

  it("compares models whatever order their keys came in", () => {
    expect(stableJson({ b: 1, a: [{ d: 2, c: 3 }] })).toBe(stableJson({ a: [{ c: 3, d: 2 }], b: 1 }));
    expect(blockDiff([para("a"), para("b")], [para("a"), para("x"), para("b")])).toEqual({ from: 1, toBefore: 1, toAfter: 2 });
    expect(blockDiff([para("a")], [para("a")])).toBeNull();
  });
});

describe("making and dissolving", () => {
  it("the blocks a selection covers", () => {
    const state = stateWith([para("a"), para("b"), para("c")]);
    expect(selectedBlockRange(state.apply(state.tr.setSelection(TextSelection.create(state.doc, startOf(state, 0), startOf(state, 1) + 1))))).toEqual({ start: 0, end: 2 });
    expect(selectedBlockRange(select(state, startOf(state, 2)))).toEqual({ start: 2, end: 3 });
  });

  it("Return to Text goes to the nearest boundary in the flow, counted without the box", () => {
    // block 1 is the text block (out of the flow)
    const tops = [0, Number.NaN, 40, 80];
    expect(nearestBoundary(tops, 120, 45)).toBe(1);
    expect(nearestBoundary(tops, 120, 79)).toBe(3);
    expect(nearestBoundary(tops, 120, 500)).toBe(4);
    expect(boundaryWithoutBox(3, 1)).toBe(2);
    expect(boundaryWithoutBox(0, 1)).toBe(0);
  });

  it("names a picture the engine's way", () => {
    const state = stateWith([para("x"), { type: "paragraph", content: [{ type: "text", text: "ab" }, { type: "image", mediaId: "a.png", width: 1, height: 1 }] }]);
    const pos = startOf(state, 1) + 2;
    expect(state.doc.nodeAt(pos)?.type.name).toBe("image");
    expect(imageRef(state.doc, pos)).toEqual({ type: "image", block: 1, inline: 1 });
    expect(NodeSelection.create(state.doc, pos).node.type.name).toBe("image");
  });
});
