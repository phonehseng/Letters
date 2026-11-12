// the Missing Fonts view: for each font the open document asks for that
// isn't on this computer, what Letters does and the one button that
// fixes it.

import type { MissingFont } from "./fontsApi";

export type MissingAction =
  /** download the font itself from the library */
  | { kind: "download-font"; label: string; family: string }
  /** download the proposed substitute */
  | { kind: "download-substitute"; label: string; family: string }
  /** put the substitute in the document in place of the missing font */
  | { kind: "replace"; label: string; from: string; to: string };

/**
 * the actions offered for one missing font, the main one first. the
 * font itself beats a substitute; Replace is only offered when the page
 * can change the document (the host passed a way to do it).
 */
export function missingActions(font: MissingFont, canReplace: boolean): MissingAction[] {
  const actions: MissingAction[] = [];
  const substitute = font.substitution;
  if (font.downloadable) {
    actions.push({ kind: "download-font", label: `Download ${font.name}`, family: font.name });
  } else if (substitute.downloadable) {
    actions.push({ kind: "download-substitute", label: `Download ${substitute.used}`, family: substitute.used });
  }
  if (canReplace && substitute.available) {
    actions.push({ kind: "replace", label: `Replace with ${substitute.used}`, from: font.name, to: substitute.used });
  }
  return actions;
}

/** the heading's count: "2 Missing Fonts" or "No Missing Fonts". */
export function missingTitle(count: number): string {
  if (count === 0) return "No Missing Fonts";
  return count === 1 ? "1 Missing Font" : `${count} Missing Fonts`;
}

/** the line under the heading. */
export function missingSummary(fonts: MissingFont[]): string {
  if (fonts.length === 0) return "Every font this document uses is on this computer.";
  const exact = fonts.filter((font) => font.substitution.metricCompatible).length;
  if (exact === fonts.length) {
    return "Each has a stand-in with the same letter widths, so lines break in the same places.";
  }
  return "Letters shows a similar font in their place. Downloading a font or its stand-in makes the document look closer to how it was written.";
}
