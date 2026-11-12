// tracked formatting changes and tracked paragraph splits and joins, in
// the forms word itself stores them, so they save as real revisions.
//
// - a formatting change is word's `w:rPrChange`: the run keeps its new
//   formatting and remembers the old one. the office model keeps it as a
//   preserved run property (`{type: "preserved", xml: "<w:rPrChange …>"}`),
//   which the docx writer puts back in its place, so here it is a
//   `preserved` mark holding that xml.
// - a split or join is a revision of the paragraph mark: `w:ins` or
//   `w:del` inside the paragraph's own `w:rPr`, which the model keeps in
//   the paragraph's `props.preserved`.

import type { Mark as PmMark, Node as PmNode } from "prosemirror-model";
import type { Mark, ParagraphData } from "./model";

export interface Revision {
  id: string;
  author: string;
  date?: string;
}

/** the marks that are formatting (what a formatting change records). */
export const FORMATTING = new Set(["characterStyle", "fontFamily", "bold", "italic", "allCaps", "smallCaps", "strike", "doubleStrike", "hidden", "color", "fontSize", "highlight", "underline", "superscript", "subscript"]);

const escape = (value: string) => value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
const unescape = (value: string) => value.replace(/&quot;/g, '"').replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");

function revisionAttributes(revision: Revision): string {
  return `w:id="${escape(revision.id)}" w:author="${escape(revision.author)}"${revision.date ? ` w:date="${escape(revision.date)}"` : ""}`;
}

/** the old formatting as word's run properties, in word's order. */
export function runPropsXml(marks: Mark[]): string {
  const parts: [number, string][] = [];
  const add = (order: number, xml: string) => parts.push([order, xml]);
  for (const mark of marks) {
    const value = (field: string) => escape(String(mark[field] ?? ""));
    switch (mark.type) {
      case "characterStyle":
        add(0, `<w:rStyle w:val="${value("styleId")}"/>`);
        break;
      case "fontFamily":
        add(1, `<w:rFonts w:ascii="${value("name")}" w:hAnsi="${value("name")}" w:cs="${value("name")}"/>`);
        break;
      case "bold":
        add(2, "<w:b/>");
        break;
      case "italic":
        add(3, "<w:i/>");
        break;
      case "allCaps":
        add(4, "<w:caps/>");
        break;
      case "smallCaps":
        add(5, "<w:smallCaps/>");
        break;
      case "strike":
        add(6, "<w:strike/>");
        break;
      case "doubleStrike":
        add(7, "<w:dstrike/>");
        break;
      case "hidden":
        add(8, "<w:vanish/>");
        break;
      case "color":
        add(9, `<w:color w:val="${value("hex")}"/>`);
        break;
      case "fontSize":
        add(10, `<w:sz w:val="${value("halfPoints")}"/>`);
        break;
      case "highlight":
        add(11, `<w:highlight w:val="${value("color")}"/>`);
        break;
      case "underline":
        add(12, `<w:u w:val="${escape(String(mark.style ?? "single"))}"/>`);
        break;
      case "superscript":
        add(13, '<w:vertAlign w:val="superscript"/>');
        break;
      case "subscript":
        add(13, '<w:vertAlign w:val="subscript"/>');
        break;
    }
  }
  return `<w:rPr>${parts.sort((a, b) => a[0] - b[0]).map(([, xml]) => xml).join("")}</w:rPr>`;
}

/** run properties back to marks (the subset runPropsXml writes). */
export function marksFromRunProps(xml: string): Mark[] {
  const marks: Mark[] = [];
  const attr = (element: string, name: string) => unescape(new RegExp(`${name}="([^"]*)"`).exec(element)?.[1] ?? "");
  for (const element of xml.match(/<w:[A-Za-z]+[^>]*\/>/g) ?? []) {
    const name = /<w:([A-Za-z]+)/.exec(element)?.[1];
    switch (name) {
      case "rStyle":
        marks.push({ type: "characterStyle", styleId: attr(element, "w:val") });
        break;
      case "rFonts":
        marks.push({ type: "fontFamily", name: attr(element, "w:ascii") || attr(element, "w:hAnsi") });
        break;
      case "b":
        marks.push({ type: "bold" });
        break;
      case "i":
        marks.push({ type: "italic" });
        break;
      case "caps":
        marks.push({ type: "allCaps" });
        break;
      case "smallCaps":
        marks.push({ type: "smallCaps" });
        break;
      case "strike":
        marks.push({ type: "strike" });
        break;
      case "dstrike":
        marks.push({ type: "doubleStrike" });
        break;
      case "vanish":
        marks.push({ type: "hidden" });
        break;
      case "color":
        marks.push({ type: "color", hex: attr(element, "w:val") });
        break;
      case "sz":
        marks.push({ type: "fontSize", halfPoints: Number(attr(element, "w:val")) });
        break;
      case "highlight":
        marks.push({ type: "highlight", color: attr(element, "w:val") });
        break;
      case "u":
        marks.push({ type: "underline", style: attr(element, "w:val") || "single" });
        break;
      case "vertAlign":
        if (attr(element, "w:val") === "superscript") marks.push({ type: "superscript" });
        if (attr(element, "w:val") === "subscript") marks.push({ type: "subscript" });
        break;
    }
  }
  return marks;
}

/** word's formatting revision: who changed it, when, and the formatting before. */
export function formatChangeXml(revision: Revision, oldMarks: Mark[]): string {
  return `<w:rPrChange ${revisionAttributes(revision)}>${runPropsXml(oldMarks.filter((mark) => FORMATTING.has(mark.type)))}</w:rPrChange>`;
}

export interface FormatChange extends Revision {
  /** the formatting before the change */
  oldMarks: Mark[];
}

/** the formatting revision a `preserved` mark holds, or null. */
export function formatChangeOf(mark: PmMark | { type: { name: string }; attrs: { data: unknown } }): FormatChange | null {
  if (mark.type.name !== "preserved") return null;
  const xml = String((mark.attrs.data as { xml?: string }).xml ?? "");
  if (!xml.startsWith("<w:rPrChange")) return null;
  const open = /^<w:rPrChange([^>]*)>/.exec(xml)?.[1] ?? "";
  const attr = (name: string) => {
    const found = new RegExp(`${name}="([^"]*)"`).exec(open)?.[1];
    return found === undefined ? undefined : unescape(found);
  };
  const inner = /<w:rPr>([\s\S]*)<\/w:rPr>/.exec(xml)?.[1] ?? "";
  return { id: attr("w:id") ?? "0", author: attr("w:author") ?? "", date: attr("w:date"), oldMarks: marksFromRunProps(inner) };
}

/** "Formatted by Ada, 2026-11-11": the tooltip on formatted text. */
export function formatChangeTitle(change: Revision): string {
  return `Formatting changed by ${change.author || "someone"}${change.date ? `, ${change.date.slice(0, 10)} ${change.date.slice(11, 16)}` : ""}`;
}

// --- paragraph marks

const MARK_REVISION = /<w:(ins|del) ([^>]*)\/>/;

/** a paragraph mark's revision: "ins" when a split made it, "del" when a join removes it. */
export function paragraphRevision(data: ParagraphData): (Revision & { kind: "ins" | "del" }) | null {
  const rpr = (data.props?.preserved ?? []).find((xml) => xml.startsWith("<w:rPr"));
  const match = rpr ? MARK_REVISION.exec(rpr) : null;
  if (!match) return null;
  const attr = (name: string) => {
    const found = new RegExp(`${name}="([^"]*)"`).exec(match[2])?.[1];
    return found === undefined ? undefined : unescape(found);
  };
  return { kind: match[1] as "ins" | "del", id: attr("w:id") ?? "0", author: attr("w:author") ?? "", date: attr("w:date") };
}

/** the paragraph with its mark recorded as inserted or deleted (one revision at a time). */
export function withParagraphRevision(data: ParagraphData, kind: "ins" | "del", revision: Revision): ParagraphData {
  const preserved = [...(data.props?.preserved ?? [])];
  const element = `<w:${kind} ${revisionAttributes(revision)}/>`;
  const index = preserved.findIndex((xml) => xml.startsWith("<w:rPr"));
  if (index < 0) preserved.push(`<w:rPr>${element}</w:rPr>`);
  else {
    const without = preserved[index].replace(new RegExp(MARK_REVISION.source, "g"), "");
    // ins and del come first in run properties
    preserved[index] = without === "<w:rPr/>" ? `<w:rPr>${element}</w:rPr>` : without.replace(/^<w:rPr>/, `<w:rPr>${element}`);
  }
  return { ...data, props: { ...(data.props ?? {}), preserved } };
}

/** the paragraph without its mark's revision. */
export function withoutParagraphRevision(data: ParagraphData): ParagraphData {
  const preserved = (data.props?.preserved ?? [])
    .map((xml) => (xml.startsWith("<w:rPr") ? xml.replace(new RegExp(MARK_REVISION.source, "g"), "") : xml))
    .filter((xml) => xml !== "<w:rPr></w:rPr>" && xml !== "<w:rPr/>");
  const props = { ...(data.props ?? {}) };
  if (preserved.length) props.preserved = preserved;
  else delete props.preserved;
  const next: ParagraphData = { ...data, props };
  if (Object.keys(props).length === 0) delete next.props;
  return next;
}

/** "New paragraph by Ada", the tooltip on a tracked paragraph mark. */
export function paragraphRevisionTitle(revision: Revision & { kind: "ins" | "del" }): string {
  const what = revision.kind === "ins" ? "Paragraph split" : "Paragraphs joined";
  return `${what} by ${revision.author || "someone"}${revision.date ? `, ${revision.date.slice(0, 10)} ${revision.date.slice(11, 16)}` : ""}`;
}

export type { PmNode };
