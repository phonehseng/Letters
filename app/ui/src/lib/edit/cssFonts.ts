// the fonts the on-page editor types in.
//
// the editor should look like the text it covers. the best is the very
// font embedded in the pdf: the shell hands it over when it is a font
// file the web view can load and it draws every letter of the text
// (`edit_font_face`). otherwise the editor uses the same font installed
// on this computer when there is one (by name), then a similar one of the
// same kind (serif, sans serif or monospaced), which is the closest the
// web view can do. this is only how the editor looks while typing: what
// the engine writes, and which font it uses, comes back as a notice.

import { editApi, type FontFamily, type FontInfo, type StandardFont } from "./editApi";
import { plainFontName } from "./editSentences";
import { TYPICAL_METRICS, type FontMetrics } from "./editorGeometry";

const GENERIC: Record<FontFamily, string> = {
  serif: '"Times New Roman", Times, serif',
  sansSerif: 'Arial, Helvetica, "Segoe UI", sans-serif',
  monospace: '"Courier New", Courier, monospace',
};

/** a font family name written for css. */
function quoted(name: string): string {
  return `"${name.replace(/["\\]/g, "")}"`;
}

/** the css stack for page text: the loaded face first, then the font by name, then its kind. */
export function fontStack(font: FontInfo, loadedFace: string | null): string {
  const named = plainFontName(font.name);
  const parts = [loadedFace ? quoted(loadedFace) : null, quoted(named), GENERIC[font.family]];
  return parts.filter(Boolean).join(", ");
}

export interface CssFontLook {
  family: string;
  weight: number;
  style: "normal" | "italic";
}

export function lookOf(font: FontInfo, loadedFace: string | null): CssFontLook {
  return { family: fontStack(font, loadedFace), weight: font.bold ? 700 : 400, style: font.italic ? "italic" : "normal" };
}

/** the twelve standard fonts, by kind and style, as the font menu offers them. */
export const STANDARD_FONTS: { font: StandardFont; label: string; family: FontFamily; bold: boolean; italic: boolean }[] = [
  { font: "helvetica", label: "Helvetica", family: "sansSerif", bold: false, italic: false },
  { font: "helveticaBold", label: "Helvetica Bold", family: "sansSerif", bold: true, italic: false },
  { font: "helveticaOblique", label: "Helvetica Oblique", family: "sansSerif", bold: false, italic: true },
  { font: "helveticaBoldOblique", label: "Helvetica Bold Oblique", family: "sansSerif", bold: true, italic: true },
  { font: "timesRoman", label: "Times", family: "serif", bold: false, italic: false },
  { font: "timesBold", label: "Times Bold", family: "serif", bold: true, italic: false },
  { font: "timesItalic", label: "Times Italic", family: "serif", bold: false, italic: true },
  { font: "timesBoldItalic", label: "Times Bold Italic", family: "serif", bold: true, italic: true },
  { font: "courier", label: "Courier", family: "monospace", bold: false, italic: false },
  { font: "courierBold", label: "Courier Bold", family: "monospace", bold: true, italic: false },
  { font: "courierOblique", label: "Courier Oblique", family: "monospace", bold: false, italic: true },
  { font: "courierBoldOblique", label: "Courier Bold Oblique", family: "monospace", bold: true, italic: true },
];

/** the standard font nearest a page font: the same kind, bold and italic. */
export function nearestStandard(font: Pick<FontInfo, "family" | "bold" | "italic">): StandardFont {
  const match = STANDARD_FONTS.find((each) => each.family === font.family && each.bold === font.bold && each.italic === font.italic);
  return match?.font ?? "helvetica";
}

export function standardLook(font: StandardFont): CssFontLook {
  const entry = STANDARD_FONTS.find((each) => each.font === font) ?? STANDARD_FONTS[0];
  return { family: GENERIC[entry.family], weight: entry.bold ? 700 : 400, style: entry.italic ? "italic" : "normal" };
}

// faces loaded from documents, by document and font id. a font that
// can't be handed over is remembered as null so it isn't asked for again
const faces = new Map<string, Promise<string | null>>();

/**
 * loads the embedded font of some text into the web view and answers the
 * family name to use, or null when it can't be used for this text.
 */
export function embeddedFace(documentId: number, font: FontInfo, sample: string): Promise<string | null> {
  if (!font.embedded || typeof FontFace === "undefined") return Promise.resolve(null);
  const key = `${documentId}:${font.id}:${[...new Set(sample)].sort().join("")}`;
  let face = faces.get(key);
  if (!face) {
    face = loadFace(documentId, font, sample, key);
    faces.set(key, face);
  }
  return face;
}

async function loadFace(documentId: number, font: FontInfo, sample: string, key: string): Promise<string | null> {
  try {
    const bytes = await editApi.fontFace(documentId, font.id, sample);
    const family = `Letters page font ${hash(key)}`;
    const face = new FontFace(family, bytes, { weight: font.bold ? "700" : "400", style: font.italic ? "italic" : "normal" });
    // the web view checks the file; many subset fonts lack tables it needs,
    // and then the installed font is used instead
    await face.load();
    document.fonts.add(face);
    return family;
  } catch {
    return null;
  }
}

function hash(text: string): string {
  let value = 0;
  for (const character of text) value = (value * 31 + character.charCodeAt(0)) >>> 0;
  return value.toString(36);
}

/** the font's ascent and descent per em, as the web view will draw it. */
export function measureMetrics(look: CssFontLook): FontMetrics {
  try {
    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
    if (!context) return TYPICAL_METRICS;
    context.font = `${look.style} ${look.weight} 100px ${look.family}`;
    const measured = context.measureText("Hxgy");
    const ascent = measured.fontBoundingBoxAscent;
    const descent = measured.fontBoundingBoxDescent;
    if (!Number.isFinite(ascent) || !Number.isFinite(descent) || ascent <= 0) return TYPICAL_METRICS;
    return { ascent: ascent / 100, descent: descent / 100 };
  } catch {
    return TYPICAL_METRICS;
  }
}

/** a colour from the engine as css. */
export function rgbCss(color: { r: number; g: number; b: number }): string {
  return `rgb(${color.r} ${color.g} ${color.b})`;
}

/** "#336699" for a colour input, and back. */
export function rgbToHex(color: { r: number; g: number; b: number }): string {
  return `#${[color.r, color.g, color.b].map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

export function hexToRgb(hex: string): { r: number; g: number; b: number } {
  const value = Number.parseInt(hex.replace("#", ""), 16);
  return { r: (value >> 16) & 255, g: (value >> 8) & 255, b: value & 255 };
}
