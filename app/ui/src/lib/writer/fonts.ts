// fonts, behind one small adapter. the font library (app/ui/src/lib/fonts,
// core/fonts) gives the Format inspector its FontPicker, loads fonts that
// aren't installed into the editor from the bytes the shell serves
// (useFontInDocument, with the FontFace api) and has the Missing Fonts
// view the status bar opens. this file keeps what the writer needs on top:
// which fonts a document uses, replacing one everywhere, and the
// metric-compatible stand-ins for Office fonts.

import type { Node as PmNode } from "prosemirror-model";
import type { EditorState, Transaction } from "prosemirror-state";
import type { DocumentMeta, StyleSheet } from "./model";

/** fonts that come with Windows and macOS, for when the system list can't be read. */
export const SAFE_FONTS = ["Arial", "Calibri", "Cambria", "Courier New", "Georgia", "Helvetica", "Times New Roman", "Verdana"];

/** every font name the document's styles and text use. */
export function documentFonts(meta: DocumentMeta, used: string[] = []): string[] {
  const names = new Set<string>(used);
  const add = (name: unknown) => typeof name === "string" && name && names.add(name);
  add(meta.styles?.defaults?.run?.font?.name);
  for (const style of meta.styles?.styles ?? []) add(style.run?.font?.name);
  add(meta.styles?.themeFonts?.major);
  add(meta.styles?.themeFonts?.minor);
  return [...names].sort((a, b) => a.localeCompare(b));
}

let systemFonts: Promise<string[]> | null = null;

/** the computer's font families, once; the safe list when they can't be read. */
export function loadSystemFonts(): Promise<string[]> {
  if (!systemFonts) {
    const query = (window as unknown as { queryLocalFonts?: () => Promise<{ family: string }[]> }).queryLocalFonts;
    systemFonts = query
      ? query()
          .then((fonts) => [...new Set(fonts.map((font) => font.family))].sort((a, b) => a.localeCompare(b)))
          .then((families) => (families.length ? families : SAFE_FONTS))
          .catch(() => SAFE_FONTS)
      : Promise.resolve(SAFE_FONTS);
  }
  return systemFonts;
}

/** the menu: the document's fonts first, then the rest, without repeats. */
export function fontChoices(documentOnes: string[], system: string[]): { group: string; names: string[] }[] {
  const inDocument = new Set(documentOnes);
  return [
    { group: "In This Document", names: documentOnes },
    { group: "On This Computer", names: system.filter((name) => !inDocument.has(name)) },
  ].filter((group) => group.names.length > 0);
}

/** open fonts with the same widths as common Office fonts, so lines break the same. */
const METRIC_SUBSTITUTES: Record<string, string> = {
  calibri: "Carlito",
  cambria: "Caladea",
  arial: "Arimo",
  helvetica: "Arimo",
  "times new roman": "Tinos",
  "courier new": "Cousine",
};

/** the metric-compatible stand-in for a font, or null when none is known. */
export function metricSubstitute(name: string): string | null {
  return METRIC_SUBSTITUTES[name.trim().toLowerCase()] ?? null;
}

/** the document's fonts this computer can't show (needs a screen: document.fonts). */
export function missingFonts(names: string[]): string[] {
  if (typeof document === "undefined") return [];
  // the web view says nothing about installed fonts, so measure: a font
  // that is there changes the width of a sample next to both fallbacks
  const context = document.createElement("canvas").getContext("2d");
  if (!context) return [];
  const sample = "mmmmmmmmmmlli WQ@#0123";
  const width = (font: string) => {
    context.font = `48px ${font}`;
    return context.measureText(sample).width;
  };
  return names.filter((name) => {
    const quoted = `"${name.replace(/"/g, "")}"`;
    return ["monospace", "serif"].every((fallback) => width(`${quoted}, ${fallback}`) === width(fallback));
  });
}

/**
 * the adapter's hook for fonts that are not installed: the library will
 * hand over the font's bytes from the shell, and they are loaded for the
 * editor without installing them. no library yet, so nothing is loaded.
 */
export async function loadFontBytes(name: string, bytes: ArrayBuffer | null): Promise<boolean> {
  if (!bytes || typeof FontFace === "undefined") return false;
  const face = new FontFace(name, bytes);
  await face.load();
  document.fonts.add(face);
  return true;
}
/** fonts the text itself sets (font marks), besides the styles'. */
export function fontsInText(doc: PmNode): string[] {
  const names = new Set<string>();
  doc.descendants((node) => {
    for (const mark of node.marks) if (mark.type.name === "fontFamily") names.add(String((mark.attrs.data as { name?: string }).name ?? ""));
    return true;
  });
  names.delete("");
  return [...names];
}

/** one font replaced by another everywhere: the text's font marks and the styles (the Missing Fonts view's Replace). one undo step. */
export function replaceFont(state: EditorState, from: string, to: string): Transaction | null {
  const tr = state.tr;
  const type = state.schema.marks.fontFamily;
  state.doc.descendants((node, pos) => {
    if (!node.isText) return true;
    const mark = node.marks.find((each) => each.type === type && (each.attrs.data as { name?: string }).name === from);
    if (mark) tr.addMark(pos, pos + node.nodeSize, type.create({ data: { ...(mark.attrs.data as object), name: to } }));
    return false;
  });
  const meta = state.doc.attrs.meta as DocumentMeta;
  const styles = renameInStyles(meta.styles, from, to);
  if (styles !== meta.styles) tr.setDocAttribute("meta", { ...meta, styles });
  return tr.docChanged || styles !== meta.styles ? tr : null;
}

function renameInStyles(sheet: StyleSheet, from: string, to: string): StyleSheet {
  let changed = false;
  const rename = <T extends { run?: { font?: { name: string } } }>(item: T): T => {
    if (item.run?.font?.name !== from) return item;
    changed = true;
    return { ...item, run: { ...item.run, font: { ...item.run.font, name: to } } };
  };
  const next = { ...sheet, defaults: sheet.defaults ? rename(sheet.defaults) : sheet.defaults, styles: sheet.styles.map(rename) };
  return changed ? next : sheet;
}