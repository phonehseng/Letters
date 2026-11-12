// content the model keeps exactly as the file had it (a chart, a text
// box, an equation...). the editor shows it as a quiet chip that names
// what it is, so nobody wonders where it went, and it is written back
// untouched on save.

const KINDS: [RegExp, string][] = [
  [/<c:chart|chartml/i, "Chart"],
  [/<(wps:txbx|v:textbox|w:txbxContent|draw:text-box)/i, "Text box"],
  [/<(m:oMath|math:math|draw:object)/i, "Equation"],
  [/<(dgm:|SmartArt)/i, "SmartArt"],
  [/<(wps:wsp|v:shape|v:rect|v:oval|draw:custom-shape|draw:rect|draw:ellipse|wpg:)/i, "Shape"],
  [/<(w:sdt|text:section)/i, "Content control"],
  [/<(text:table-of-content|w:instrText[^>]*>\s*TOC)/i, "Table of contents"],
  [/<(w:object|o:OLEObject|draw:frame)/i, "Embedded object"],
  [/<w:sym\b/i, "Symbol"],
  [/<(w:fldSimple|w:fldChar|text:[a-z-]+-field)/i, "Field"],
];

/** what a piece of preserved xml is, in a word people know. */
export function preservedKind(xml: string): string {
  for (const [pattern, kind] of KINDS) if (pattern.test(xml)) return kind;
  return "Item";
}

/** the chip's words: "Chart — kept as it is". */
export function preservedLabel(xml: string): string {
  return `${preservedKind(xml)} — kept as it is`;
}

/** the chip's tooltip, which also says how to delete it on purpose. */
export function preservedTooltip(xml: string): string {
  return `${preservedKind(xml)} from the original file. Letters can’t edit it, so it is saved back exactly as it was. To remove it, click it and press Delete.`;
}
