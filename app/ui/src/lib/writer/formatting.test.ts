// styles, marks, paragraph formatting, lists, indents and inserting.
import { describe, expect, it } from "vitest";
import { NodeSelection } from "prosemirror-state";
import { docToModel } from "./convert";
import {
  addComment,
  applyStyle,
  backspaceIndent,
  deleteComment,
  insertFootnote,
  insertImage,
  insertPageBreak,
  insertTable,
  markActive,
  metaOf,
  placementFor,
  setLineSpacing,
  setOrientation,
  setPageSize,
  shiftTabCommand,
  tabCommand,
  toggleList,
  toggleMark,
  updateImage,
} from "./editing";
import { indentAfterShiftTab, indentAfterTab, listLevelAfter } from "./indentation";
import { ListCounter, formatNumber, listFor } from "./numbering";
import { schema } from "./schema";
import { choiceForParagraph, documentStyleChoices, headingStyle, resolvedParagraphProps, styleHeadingLevel } from "./styles";
import wordMade from "./fixtures/word-made.json";
import type { DocumentModel } from "./model";
import { para, select, startOf, stateWith } from "./testDocs";

const blocksOf = (state: { doc: Parameters<typeof docToModel>[0] }) => docToModel(state.doc).blocks;

describe("easy headers", () => {
  it("Heading 1 makes a heading with the document's heading style, created if missing", () => {
    let state = select(stateWith([para("Intro")]), 2);
    state = state.apply(applyStyle(state, "h1")!);
    expect(blocksOf(state)[0]).toMatchObject({ type: "heading", level: 1, styleId: "Heading1" });
    expect(metaOf(state).styles.styles.some((style) => style.id === "Heading1" && style.paragraph?.outlineLevel === 0)).toBe(true);
    // back to body text
    state = state.apply(applyStyle(state, "body")!);
    expect(blocksOf(state)[0]).toEqual({ type: "paragraph", content: [{ type: "text", text: "Intro" }] });
  });

  it("uses the styles word made, and finds Title and Quote", () => {
    const model = wordMade as unknown as DocumentModel;
    expect(headingStyle(model.styles, 2).id).toBe("Heading2");
    expect(styleHeadingLevel(model.styles, "Heading3")).toBe(3);
    let state = select(stateWith([para("A quote")], { styles: model.styles }), 2);
    state = state.apply(applyStyle(state, "quote")!);
    expect(blocksOf(state)[0]).toMatchObject({ type: "paragraph", styleId: "Quote" });
    expect(choiceForParagraph(model.styles, "Quote", null)).toBe("quote");
    expect(choiceForParagraph(model.styles, undefined, 2)).toBe("h2");
    expect(documentStyleChoices(model.styles).map((choice) => choice.label)).toContain("List Paragraph");
  });
});

describe("marks and paragraphs", () => {
  it("bold toggles on and off over a selection", () => {
    let state = select(stateWith([para("Make bold")]), 1, 5);
    state = state.apply(toggleMark(state, "bold")!);
    expect(markActive(state, schema.marks.bold)).toBe(true);
    expect(blocksOf(state)[0].content).toEqual([{ type: "text", text: "Make", marks: [{ type: "bold" }] }, { type: "text", text: " bold" }]);
    state = state.apply(toggleMark(state, "bold")!);
    expect(markActive(state, schema.marks.bold)).toBe(false);
  });

  it("line spacing is stored the way word stores it", () => {
    let state = select(stateWith([para("Double")]), 2);
    state = state.apply(setLineSpacing(state, 2)!);
    expect(blocksOf(state)[0]).toMatchObject({ props: { spacing: { line: { rule: "auto", value: 480 } } } });
  });
});

describe("smart indentation", () => {
  it("Tab at the start of a paragraph gives a first-line indent, then indents the paragraph", () => {
    expect(indentAfterTab({})).toEqual({ firstLine: 720, hanging: undefined });
    expect(indentAfterTab({ firstLine: 720 })).toEqual({ firstLine: 720, left: 720 });
    expect(indentAfterTab({ left: 720, hanging: 720 })).toEqual({ left: 1440, hanging: 720 });
    expect(indentAfterShiftTab({ firstLine: 720, left: 720 })).toEqual({ firstLine: 720, left: 0 });
    expect(indentAfterShiftTab({ firstLine: 720 })).toEqual({ firstLine: 0 });
    expect(indentAfterShiftTab({})).toBeNull();
    expect(listLevelAfter({ listId: "1", level: 0 }, "out")).toBeNull();
  });

  it("works through the editor, and Tab inside text types a tab", () => {
    let state = select(stateWith([para("Paragraph")]), 1);
    state = state.apply(tabCommand(state)!);
    expect(blocksOf(state)[0]).toMatchObject({ props: { indent: { firstLine: 720 } } });
    state = state.apply(backspaceIndent(state)!);
    expect(blocksOf(state)[0]).toMatchObject({ props: { indent: { firstLine: 0 } } });
    expect(tabCommand(select(state, 4))).toBeNull();
    expect(shiftTabCommand(select(stateWith([para("x")]), 1))).toBeNull();
  });

  it("in a list Tab and Shift Tab change the level", () => {
    let state = select(stateWith([para("Item")]), 1);
    state = state.apply(toggleList(state, "bullet")!);
    state = state.apply(tabCommand(state)!);
    expect(blocksOf(state)[0]).toMatchObject({ props: { list: { level: 1 } } });
    state = state.apply(shiftTabCommand(state)!);
    expect(blocksOf(state)[0]).toMatchObject({ props: { list: { level: 0 } } });
  });
});

describe("lists", () => {
  it("bullets on, numbers carry on from the item before, and off again", () => {
    let state = stateWith([para("One"), para("Two")]);
    state = select(state, startOf(state, 0), startOf(state, 1) + 2);
    state = state.apply(toggleList(state, "number")!);
    const meta = metaOf(state);
    const counter = new ListCounter(meta.numbering);
    const labels = docToModel(state.doc).blocks.map((block) => {
      const list = (block.props as { list: { listId: string; level: number } }).list;
      return counter.next(list.listId, list.level);
    });
    expect(labels).toEqual(["1.", "2."]);
    state = state.apply(toggleList(state, "number")!);
    expect(blocksOf(state)[0]).toEqual({ type: "paragraph", content: [{ type: "text", text: "One" }] });
  });

  it("counts like word: levels restart under a new parent", () => {
    const { numbering, listId } = listFor({ definitions: [], instances: [] }, "number", null);
    const counter = new ListCounter(numbering);
    expect([counter.next(listId, 0), counter.next(listId, 1), counter.next(listId, 1), counter.next(listId, 0), counter.next(listId, 1)]).toEqual([
      "1.", "a.", "b.", "2.", "a.",
    ]);
    expect(formatNumber(1994, "upperRoman")).toBe("MCMXCIV");
    expect(formatNumber(27, "lowerLetter")).toBe("aa");
  });

  it("the word-made list resolves through its style", () => {
    const model = wordMade as unknown as DocumentModel;
    const item = model.blocks[3] as unknown as { styleId: string; props: { list: { listId: string; level: number } } };
    expect(resolvedParagraphProps(model, item.styleId, item.props).list?.listId).toBe("1");
    expect(new ListCounter(model.numbering).next("1", 0)).toBe("1.");
  });
});

describe("inserting", () => {
  it("a table puts the cursor in its first cell", () => {
    let state = select(stateWith([para("")]), 1);
    state = state.apply(insertTable(state, 2, 3));
    const table = blocksOf(state).find((block) => block.type === "table") as unknown as { rows: { cells: unknown[] }[]; columnWidths: number[] };
    expect(table.rows).toHaveLength(2);
    expect(table.rows[0].cells).toHaveLength(3);
    expect(table.columnWidths).toEqual([3120, 3120, 3120]);
    expect(state.selection.$from.parent.type.name).toBe("paragraph");
    expect(state.selection.$from.node(-1).type.name).toBe("table_cell");
  });

  it("a page break splits the paragraph", () => {
    let state = select(stateWith([para("BeforeAfter")]), 7);
    state = state.apply(insertPageBreak(state));
    expect(blocksOf(state).map((block) => block.type)).toEqual(["paragraph", "pageBreak", "paragraph"]);
    expect(state.doc.child(2).textContent).toBe("After");
  });

  it("a footnote adds a reference and a note", () => {
    let state = select(stateWith([para("Claim")]), 6);
    const { tr, noteId } = insertFootnote(state, "Source.");
    state = state.apply(tr);
    expect(metaOf(state).notes.find((note) => note.id === noteId)?.blocks[0]).toMatchObject({ content: [{ text: "Source." }] });
    expect(blocksOf(state)[0].content).toContainEqual({ type: "noteReference", noteId, marks: [{ type: "superscript" }] });
  });

  it("a comment marks the text and can be deleted again", () => {
    let state = select(stateWith([para("Look here")]), 1, 5);
    const added = addComment(state, "Ada Lovelace", "Why?", "2026-11-12T10:00:00Z")!;
    state = state.apply(added.tr);
    expect(metaOf(state).comments[0]).toMatchObject({ author: "Ada Lovelace", initials: "AL" });
    expect(blocksOf(state)[0].content).toContainEqual({ type: "commentReference", commentId: added.commentId });
    state = state.apply(deleteComment(state, added.commentId));
    expect(blocksOf(state)[0].content).toEqual([{ type: "text", text: "Look here" }]);
    expect(metaOf(state).comments).toEqual([]);
  });

  it("a picture can be resized and its wrap and alt text changed", () => {
    let state = select(stateWith([para("")]), 1);
    state = state.apply(insertImage(state, "letters-picture-1.png", 2880, 1440, "photo.png"));
    let pos = -1;
    state.doc.descendants((node, at) => {
      if (node.type.name === "image") pos = at;
    });
    state = state.apply(state.tr.setSelection(NodeSelection.create(state.doc, pos)));
    state = state.apply(updateImage(state, (data) => ({ ...data, description: "A graph", placement: placementFor("square", data.placement as never) }))!);
    const image = blocksOf(state)[0].content as { description: string; placement: { type: string; wrap: string } }[];
    expect(image[0]).toMatchObject({ description: "A graph", placement: { type: "floating", wrap: "square" } });
  });

  it("page size and orientation change every section", () => {
    let state = stateWith([para("x")]);
    state = state.apply(setPageSize(state, "a4"));
    expect(metaOf(state).sections[0].page).toMatchObject({ width: 11906, height: 16838 });
    state = state.apply(setOrientation(state, "landscape"));
    expect(metaOf(state).sections[0].page).toMatchObject({ width: 16838, height: 11906, orientation: "landscape" });
  });
});
