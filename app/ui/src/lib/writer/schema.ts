// the editor's schema: what a document may contain, mirroring the office
// model (core/office/src/model) one to one.
//
// every node and mark keeps the model object it came from in an attribute
// called `data` (minus its children), so the converter can give back
// exactly what it was given, unknown fields and all. editing commands
// replace `data` with a changed copy; they never change it in place.
//
// marks are listed in the same order as the engine's Mark::rank, because
// prosemirror keeps a node's marks in schema order: that way the order of
// marks in the model survives a round trip without sorting.

import { Schema, type DOMOutputSpec, type Mark as PmMark, type MarkSpec, type Node as PmNode } from "prosemirror-model";
import { colour, fontStack, highlightColour, paragraphClass, paragraphCss } from "./css";
import type { ParagraphData } from "./model";
import { knownMediaUrls } from "./images";
import { preservedLabel, preservedTooltip } from "./preserved";
import { formatChangeOf, formatChangeTitle, paragraphRevision, paragraphRevisionTitle } from "./trackedFormatting";

type Data = Record<string, unknown>;

const json = (value: unknown) => JSON.stringify(value ?? {});

function readJson(element: HTMLElement): Data {
  try {
    return JSON.parse(element.getAttribute("data-json") ?? "{}") as Data;
  } catch {
    return {};
  }
}

const dataAttr = { data: { default: {} } };

function paragraphDom(node: PmNode, tag: string): DOMOutputSpec {
  const data = node.attrs.data as ParagraphData;
  const attrs: Record<string, string> = { class: paragraphClass(data.styleId), style: paragraphCss(data.props), "data-json": json(data) };
  // a tracked split or join shows its paragraph mark, with who and when on hover
  const revision = paragraphRevision(data);
  if (revision) {
    attrs.class += revision.kind === "ins" ? " w-mark-inserted" : " w-mark-deleted";
    attrs["data-revision"] = paragraphRevisionTitle(revision);
  }
  return [tag, attrs, 0];
}

/** an atom that shows a small chip with words, like a preserved item. */
function chip(className: string, label: string, title: string, data: Data, inline: boolean): DOMOutputSpec {
  return [inline ? "span" : "div", { class: `w-chip ${className}`, title, "data-json": json(data), contenteditable: "false" }, label];
}

const atomInline = { inline: true, group: "inline", atom: true, selectable: true, attrs: dataAttr };

type MarkRender = (data: Data) => [string, Record<string, string>?];
type ParseRule = { tag?: string; style?: string; getAttrs?: (element: HTMLElement) => Data };

function markSpec(name: string, render: MarkRender, parse: ParseRule[] = []) {
  return {
    attrs: dataAttr,
    toDOM: (mark: PmMark): DOMOutputSpec => {
      const [tag, attrs = {}] = render(mark.attrs.data as Data);
      return [tag, { ...attrs, "data-mark": name, "data-json": json(mark.attrs.data) }, 0];
    },
    parseDOM: [
      // marks copied inside the editor come back with their data
      { tag: `[data-mark="${name}"]`, priority: 60, getAttrs: (element: HTMLElement) => ({ data: readJson(element) }) },
      ...parse.map((rule) => ({ ...rule, getAttrs: rule.getAttrs ?? (() => ({ data: {} })) })),
    ],
  };
}

const changeTitle = (data: Data) =>
  `${data.kind === "insertion" ? "Inserted" : "Deleted"} by ${String(data.author || "someone")}${data.date ? `, ${String(data.date).slice(0, 10)}` : ""}`;

/** the marks in the engine's rank order (see the note at the top). */
const MARKS: [string, MarkRender, ParseRule[]?, Record<string, unknown>?][] = [
  ["characterStyle", (data) => ["span", { "data-char-style": String(data.styleId ?? "") }]],
  ["fontFamily", (data) => ["span", { style: `font-family:${fontStack(String(data.name ?? ""))}` }]],
  ["bold", () => ["strong"], [{ tag: "strong" }, { tag: "b" }, { style: "font-weight=bold" }]],
  ["italic", () => ["em"], [{ tag: "em" }, { tag: "i" }, { style: "font-style=italic" }]],
  ["allCaps", () => ["span", { style: "text-transform:uppercase" }]],
  ["smallCaps", () => ["span", { style: "font-variant:small-caps" }]],
  ["strike", () => ["s"], [{ tag: "s" }, { tag: "strike" }]],
  ["doubleStrike", () => ["s", { style: "text-decoration-style:double" }]],
  ["hidden", () => ["span", { class: "w-hidden", title: "Hidden text" }]],
  ["color", (data) => ["span", { style: `color:${colour(String(data.hex ?? ""))}` }]],
  ["fontSize", (data) => ["span", { style: `font-size:${Number(data.halfPoints ?? 24) / 2}pt` }]],
  ["highlight", (data) => ["mark", { style: `background:${highlightColour(String(data.color ?? ""))}` }]],
  [
    "underline",
    (data): [string, Record<string, string>] => ["u", data.style && data.style !== "single" ? { style: `text-decoration-style:${underlineStyle(String(data.style))}` } : {}],
    [{ tag: "u", getAttrs: () => ({ data: { style: "single" } }) }],
  ],
  ["superscript", () => ["sup"], [{ tag: "sup:not(.w-note-ref)" }], { excludes: "superscript subscript" }],
  ["subscript", () => ["sub"], [{ tag: "sub" }], { excludes: "superscript subscript" }],
  ["language", (data) => ["span", { lang: String(data.tag ?? "") }]],
  ["off", (data) => ["span", { "data-off": String(data.property ?? "") }], [], { excludes: "" }],
  [
    "preserved",
    (data) => {
      // word's tracked formatting change is kept as preserved xml: show it as a change
      const change = formatChangeOf({ type: { name: "preserved" }, attrs: { data } });
      return change ? ["span", { class: "w-format-change", title: formatChangeTitle(change) }] : ["span", { class: "w-preserved-format" }];
    },
    [],
    { excludes: "" },
  ],
  [
    "link",
    (data) => ["a", { href: String(data.href ?? (data.anchor ? `#${data.anchor}` : "")), title: String(data.tooltip ?? data.href ?? ""), class: "w-link" }],
    [{ tag: "a[href]", getAttrs: (element) => ({ data: { href: element.getAttribute("href") ?? "" } }) }],
    { inclusive: false },
  ],
  // tracked insertions and deletions share one mark type so that text both
  // inserted and deleted keeps its two marks in the order the file had them
  ["change", (data) => [data.kind === "insertion" ? "ins" : "del", { class: `w-${String(data.kind)}`, title: changeTitle(data) }], [], { excludes: "", inclusive: false }],
  ["comment", (data) => ["span", { class: "w-comment", "data-comment": String(data.id ?? "") }], [], { excludes: "", inclusive: false }],
];

export const schema = new Schema({
  nodes: {
    doc: { content: "block+", attrs: { meta: { default: {} }, emptyBody: { default: false } } },
    paragraph: {
      group: "block",
      content: "inline*",
      attrs: dataAttr,
      parseDOM: [{ tag: "p", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => paragraphDom(node, "p"),
    },
    heading: {
      group: "block",
      content: "inline*",
      defining: true,
      attrs: { level: { default: 1 }, data: { default: {} } },
      parseDOM: [1, 2, 3, 4, 5, 6].map((level) => ({
        tag: `h${level}`,
        getAttrs: (element: HTMLElement) => ({ level, data: readJson(element) }),
      })),
      toDOM: (node) => paragraphDom(node, `h${Math.min(Math.max(node.attrs.level as number, 1), 6)}`),
    },
    table: {
      group: "block",
      content: "table_row+",
      tableRole: "table",
      isolating: true,
      attrs: dataAttr,
      parseDOM: [{ tag: "table", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => ["table", { class: "w-table", "data-json": json(node.attrs.data) }, ["tbody", 0]],
    },
    table_row: {
      content: "(table_cell | table_header)*",
      tableRole: "row",
      attrs: dataAttr,
      parseDOM: [{ tag: "tr", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => ["tr", { "data-json": json(node.attrs.data), class: (node.attrs.data as Data).header ? "w-header-row" : "" }, 0],
    },
    table_cell: cellSpec("td"),
    table_header: cellSpec("th"),
    page_break: {
      group: "block",
      atom: true,
      selectable: true,
      attrs: dataAttr,
      parseDOM: [{ tag: "div.w-page-break" }],
      toDOM: () => ["div", { class: "w-page-break", contenteditable: "false" }, ["span", "Page Break"]],
    },
    section_break: {
      group: "block",
      atom: true,
      selectable: true,
      attrs: dataAttr,
      parseDOM: [{ tag: "div.w-section-break" }],
      toDOM: () => ["div", { class: "w-section-break", contenteditable: "false" }, ["span", "Section Break"]],
    },
    // a text block: blocks of text in a frame of their own, placed on the
    // page like a free picture (model `textBox`). its content is ordinary
    // blocks, so styles, lists and tables work inside it. where it sits is
    // set by the object layer (objectLayer.ts) with a node decoration
    text_box: {
      group: "block",
      content: "block+",
      isolating: true,
      attrs: dataAttr,
      parseDOM: [{ tag: "div.w-text-box", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => ["div", { class: "w-text-box", style: textBoxCss(node.attrs.data as Data), "data-json": json(node.attrs.data) }, 0],
    },
    preserved_block: {
      group: "block",
      atom: true,
      selectable: true,
      attrs: dataAttr,
      parseDOM: [{ tag: "div.w-preserved", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => {
        const xml = String((node.attrs.data as Data).xml ?? "");
        return chip("w-preserved", preservedLabel(xml), preservedTooltip(xml), node.attrs.data as Data, false);
      },
    },
    text: { group: "inline" },
    tab: {
      ...atomInline,
      leafText: () => "\t",
      parseDOM: [{ tag: "span.w-tab", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => ["span", { class: "w-tab", "data-json": json(node.attrs.data) }, "\t"],
    },
    hard_break: {
      ...atomInline,
      leafText: () => "\n",
      parseDOM: [{ tag: "br", getAttrs: () => ({ data: { kind: "line" } }) }],
      toDOM: (node) => {
        const kind = (node.attrs.data as Data).kind;
        if (kind === "line") return ["br", { "data-json": json(node.attrs.data) }];
        return ["span", { class: "w-inline-break", "data-json": json(node.attrs.data), title: kind === "page" ? "Page break" : "Column break" }, ["br"]];
      },
    },
    image: {
      ...atomInline,
      draggable: true,
      parseDOM: [{ tag: "img[data-json]", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      // the picture's address is filled in by the image node view (images.ts)
      toDOM: (node) => ["img", { "data-json": json(node.attrs.data), alt: String((node.attrs.data as Data).description ?? ""), src: knownMediaUrls.get(String((node.attrs.data as Data).mediaId)) ?? "" }],
    },
    note_ref: {
      ...atomInline,
      parseDOM: [{ tag: "sup.w-note-ref", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => ["sup", { class: "w-note-ref", "data-json": json(node.attrs.data), "data-note": String((node.attrs.data as Data).noteId) }],
    },
    field: {
      ...atomInline,
      leafText: (node) => String((node.attrs.data as Data).result ?? ""),
      parseDOM: [{ tag: "span.w-field", getAttrs: (element) => ({ data: readJson(element as HTMLElement) }) }],
      toDOM: (node) => {
        const data = node.attrs.data as Data;
        return ["span", { class: "w-field", "data-json": json(data), "data-kind": String(data.kind), title: fieldTitle(String(data.kind)) }, String(data.result ?? "")];
      },
    },
    bookmark_start: { ...atomInline, marks: "", selectable: false, toDOM: (node) => ["span", { class: "w-bookmark", "data-json": json(node.attrs.data) }] },
    bookmark_end: { ...atomInline, marks: "", selectable: false, toDOM: (node) => ["span", { class: "w-bookmark", "data-json": json(node.attrs.data) }] },
    comment_ref: {
      ...atomInline,
      marks: "",
      toDOM: (node) => ["span", { class: "w-comment-ref", title: "Comment", "data-json": json(node.attrs.data), "data-comment": String((node.attrs.data as Data).commentId) }],
    },
    preserved_run: {
      ...atomInline,
      toDOM: (node) => {
        const xml = String((node.attrs.data as Data).xml ?? "");
        return chip("w-preserved", preservedLabel(xml), preservedTooltip(xml), node.attrs.data as Data, true);
      },
    },
    preserved_inline: {
      ...atomInline,
      marks: "",
      toDOM: (node) => {
        const xml = String((node.attrs.data as Data).xml ?? "");
        return chip("w-preserved", preservedLabel(xml), preservedTooltip(xml), node.attrs.data as Data, true);
      },
    },
  },
  marks: Object.fromEntries(MARKS.map(([name, render, parse, extra]) => [name, { ...markSpec(name, render, parse), ...extra }])) as Record<string, MarkSpec>,
});

/** a text block's frame: its size, inner margins, line and fill, from the model. */
export function textBoxCss(data: Data): string {
  const px = (twips: unknown) => `${(Number(twips) || 0) / 15}px`;
  const margins = (data.margins ?? { left: 144, top: 72, right: 144, bottom: 72 }) as Record<string, number>;
  const border = data.border as { style?: string; size?: number; color?: string } | undefined;
  const line = border && border.style !== "none" ? `${Math.max(0.5, (border.size ?? 4) / 6)}px solid ${colour(border.color ?? "000000")}` : "1px dashed rgb(0 0 0 / 0.18)";
  return [
    `width:${px(data.width)}`,
    data.autoHeight ? `min-height:${px(data.height)}` : `height:${px(data.height)}`,
    `padding:${px(margins.top)} ${px(margins.right)} ${px(margins.bottom)} ${px(margins.left)}`,
    `border:${line}`,
    `background:${data.fill ? colour(String(data.fill)) : "transparent"}`,
  ].join(";");
}

function underlineStyle(style: string): string {
  if (style === "double") return "double";
  if (style === "wave" || style.startsWith("wavy")) return "wavy";
  if (style === "dotted" || style.startsWith("dot")) return "dotted";
  if (style.startsWith("dash")) return "dashed";
  return "solid";
}

function fieldTitle(kind: string): string {
  if (kind === "pageNumber") return "Page number. Word and other apps fill it in for each page.";
  if (kind === "pageCount") return "Number of pages, filled in when the document is printed or exported.";
  if (kind === "date") return "A date the document fills in itself.";
  return "A table of contents. Update it in Word to fill it in.";
}

function cellSpec(tag: "td" | "th") {
  return {
    content: "block+",
    tableRole: tag === "td" ? "cell" : "header_cell",
    isolating: true,
    attrs: { colspan: { default: 1 }, rowspan: { default: 1 }, colwidth: { default: null }, data: { default: {} } },
    parseDOM: [
      {
        tag,
        getAttrs: (element: HTMLElement) => ({
          colspan: Number(element.getAttribute("colspan") ?? 1),
          rowspan: Number(element.getAttribute("rowspan") ?? 1),
          data: readJson(element),
        }),
      },
    ],
    toDOM: (node: PmNode): DOMOutputSpec => {
      const data = node.attrs.data as Data;
      const style = [data.shading ? `background:${colour(String(data.shading))}` : "", data.verticalAlign ? `vertical-align:${data.verticalAlign === "center" ? "middle" : data.verticalAlign}` : ""]
        .filter(Boolean)
        .join(";");
      const attrs: Record<string, string> = { "data-json": json(data), style };
      if (node.attrs.colspan !== 1) attrs.colspan = String(node.attrs.colspan);
      if (node.attrs.rowspan !== 1) attrs.rowspan = String(node.attrs.rowspan);
      return [tag, attrs, 0];
    },
  };
}

export type { PmNode };
