// fonts, behind one small adapter. the font library (app/ui/src/lib/fonts,
// core/fonts) is being built by another engineer: when it lands, its
// FontPicker, its fonts served as bytes (loaded with the FontFace api) and
// its Missing Fonts view replace the fallbacks here. until then:
//
// the fonts the Font menu offers: the document's own, then the computer's
// (through the Local Font Access API where the web view allows it), or a
// short list of fonts nearly every computer has.

import type { DocumentMeta } from "./model";

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