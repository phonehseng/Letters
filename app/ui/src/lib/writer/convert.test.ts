import { describe, expect, it } from "vitest";
import { blocksToDoc, docToBlocks, docToModel, modelToDoc } from "./convert";
import type { Block, DocumentModel } from "./model";
import { emptyModel } from "./model";
import wordMade from "./fixtures/word-made.json";
import wordMadeAsOdt from "./fixtures/word-made-as-odt.json";

/** model -> editor -> model, checking the editor document is valid on the way. */
function roundTrip(model: DocumentModel): DocumentModel {
  const doc = modelToDoc(model);
  doc.check();
  return docToModel(doc);
}

function withBlocks(blocks: Block[]): DocumentModel {
  return { ...emptyModel(), blocks };
}

const text = (value: string, marks?: unknown[]) => (marks ? { type: "text", text: value, marks } : { type: "text", text: value });

describe("model to editor and back", () => {
  it("gives back a document word made exactly, ids, styles, lists, notes and all", () => {
    expect(roundTrip(wordMade as unknown as DocumentModel)).toEqual(wordMade);
  });

  it("gives back the same document after a trip through odt", () => {
    expect(roundTrip(wordMadeAsOdt as unknown as DocumentModel)).toEqual(wordMadeAsOdt);
  });

  it("keeps an empty body empty", () => {
    expect(roundTrip(withBlocks([]))).toEqual(withBlocks([]));
  });

  it("keeps paragraph formatting, style ids and list membership", () => {
    const model = withBlocks([
      {
        type: "paragraph",
        styleId: "ListParagraph",
        props: { alignment: "justify", indent: { left: 720, hanging: 360 }, spacing: { before: 0, line: { rule: "auto", value: 480 } }, list: { listId: "3", level: 1 } },
        attributes: [{ name: "w14:paraId", value: "1A2B3C4D" }],
        content: [text("Item")],
      },
      { type: "heading", level: 4, styleId: "Heading4", content: [text("Deep")] },
    ]);
    expect(roundTrip(model)).toEqual(model);
  });

  it("keeps marks in their order, including several of the same kind", () => {
    const marks = [
      { type: "characterStyle", styleId: "Emphasis" },
      { type: "fontFamily", name: "Arial", theme: "minorHAnsi", extra: [{ name: "w:eastAsia", value: "MS Mincho" }] },
      { type: "bold" },
      { type: "color", hex: "FF0000" },
      { type: "fontSize", halfPoints: 28 },
      { type: "highlight", color: "yellow" },
      { type: "underline", style: "double" },
      { type: "superscript" },
      { type: "off", property: "italic" },
      { type: "off", property: "smallCaps" },
      { type: "preserved", xml: "<w:spacing w:val=\"20\"/>" },
      { type: "preserved", xml: "<w:kern w:val=\"16\"/>" },
      { type: "link", href: "https://example.com", tooltip: "Example" },
      { type: "deletion", id: "7", author: "B", date: "2026-11-01T10:00:00Z" },
      { type: "insertion", id: "6", author: "A" },
      { type: "comment", id: "1" },
      { type: "comment", id: "2" },
    ];
    const model = withBlocks([{ type: "paragraph", content: [text("Formatted", marks)] }]);
    expect(roundTrip(model)).toEqual(model);
  });

  it("keeps every kind of inline content", () => {
    const model = withBlocks([
      {
        type: "paragraph",
        content: [
          { type: "bookmarkStart", id: "0", name: "_Toc1" },
          text("A"),
          { type: "tab", marks: [{ type: "bold" }] },
          { type: "break", kind: "line" },
          { type: "break", kind: "page" },
          { type: "noteReference", noteId: "fn1", marks: [{ type: "characterStyle", styleId: "FootnoteReference" }] },
          { type: "field", kind: "pageNumber", instruction: "PAGE", result: "1" },
          { type: "commentReference", commentId: "1" },
          { type: "preservedRun", xml: "<w:sym w:font=\"Symbol\" w:char=\"F0B7\"/>", marks: [{ type: "italic" }] },
          { type: "preserved", xml: "<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>" },
          { type: "image", mediaId: "image1.png", width: 1440, height: 720, description: "A chart", placement: { type: "floating", horizontal: { relativeTo: "column", offset: 0 }, vertical: { relativeTo: "paragraph", offset: 10 }, wrap: "square" } },
          { type: "bookmarkEnd", id: "0" },
        ],
      },
    ]);
    expect(roundTrip(model)).toEqual(model);
  });

  it("keeps tables with spans, empty cells and their properties", () => {
    const model = withBlocks([
      {
        type: "table",
        styleId: "TableGrid",
        columnWidths: [1000, 2000, 3000],
        rows: [
          {
            header: true,
            cantSplit: false,
            cells: [
              { colSpan: 2, rowSpan: 1, shading: "D9D9D9", blocks: [{ type: "paragraph", content: [text("Wide")] }] },
              { colSpan: 1, rowSpan: 2, verticalAlign: "center", blocks: [] },
            ],
          },
          {
            header: false,
            cantSplit: true,
            height: 400,
            cells: [
              { colSpan: 1, rowSpan: 1, blocks: [{ type: "paragraph", content: [] }] },
              { colSpan: 1, rowSpan: 1, blocks: [{ type: "table", columnWidths: [500], rows: [{ header: false, cantSplit: false, cells: [{ colSpan: 1, rowSpan: 1, blocks: [{ type: "paragraph", content: [text("Nested")] }] }] }] }] },
            ],
          },
        ],
      },
    ]);
    expect(roundTrip(model)).toEqual(model);
  });

  it("keeps breaks, preserved blocks and block kinds it has never seen", () => {
    const model = withBlocks([
      { type: "pageBreak" },
      { type: "sectionBreak" },
      { type: "preserved", xml: "<w:sdt><w:sdtContent/></w:sdt>" },
      { type: "futureBlock", something: [1, 2, 3] },
      { type: "paragraph", content: [{ type: "futureInline", value: true } as never, text("x", [{ type: "futureMark", strength: 3 }])] },
    ]);
    expect(roundTrip(model)).toEqual(model);
  });

  it("keeps tracked changes as marks on the text", () => {
    const model = withBlocks([
      {
        type: "paragraph",
        content: [text("Kept "), text("added", [{ type: "insertion", id: "1", author: "Ada", date: "2026-11-11T09:00:00Z" }]), text(" removed", [{ type: "deletion", id: "2", author: "Ada" }])],
      },
    ]);
    expect(roundTrip(model)).toEqual(model);
  });

  it("converts header, footer and note blocks on their own", () => {
    const blocks: Block[] = [{ type: "paragraph", props: { alignment: "right" }, content: [text("Lovelace "), { type: "field", kind: "pageNumber", instruction: "PAGE", result: "1" }] }];
    expect(docToBlocks(blocksToDoc(blocks))).toEqual(blocks);
    expect(docToBlocks(blocksToDoc([]))).toEqual([]);
  });
});
