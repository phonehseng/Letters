// the sentences of the Edit tab: what happened to the font, what happens
// when new text no longer fits, and the "what will happen" lines under
// each button. kept apart from the components so they are tested and
// read as a set (docs/DESIGN.md: short, plain, never "oops").

import type { FontInfo, FontNotice, TextOverflow } from "./editApi";

/** “é”, or “é” and “ß”, or “é”, “ß” and “ñ”. */
export function quotedList(characters: string): string {
  const each = [...characters].map((character) => `“${character}”`);
  if (each.length <= 1) return each.join("");
  return `${each.slice(0, -1).join(", ")} and ${each[each.length - 1]}`;
}

/** a font's name as people know it: "Arial-BoldMT" is Arial. */
export function plainFontName(name: string): string {
  const base = name.replace(/^[A-Z]{6}\+/, "").split(/[-,]/)[0];
  // "PSMT" and "MT" are the font makers' tags, not part of the name people know
  const spaced = base.replace(/(PSMT|MT|PS|Std|Pro)$/, "").replace(/([a-z])([A-Z])/g, "$1 $2");
  return spaced.trim() || name;
}

/**
 * what the engine did for characters the original font can't draw, in
 * plain words. never a silent substitution: this is shown beside the text.
 */
export function noticeSentence(notice: FontNotice): string {
  const characters = quotedList(notice.characters);
  const it = [...notice.characters].length === 1 ? "it" : "them";
  const original = "The original font";
  if (notice.kind === "installedFont") {
    return `${original} can’t draw ${characters}, so ${notice.usedFont} from this computer was used for ${it}.`;
  }
  if (notice.message.includes("doesn’t allow being copied")) {
    // the engine explains a refused installed copy; its own words are clearest
    return notice.message;
  }
  return `${original} can’t draw ${characters}, so ${notice.usedFont}, a font every PDF reader has, was used for ${it}.`;
}

/** what Let It Grow and Shrink to Fit are about. */
export function overflowSentence(overflow: TextOverflow): string {
  const more = overflow.linesAfter - overflow.linesBefore;
  const lines = more === 1 ? "1 more line" : `${more} more lines`;
  const lower = Math.round(overflow.extraHeight);
  return `The new text needs ${lines} than the paragraph had, so it now reaches ${lower} pt lower and may cover what is below.`;
}

/** said while typing, before anything is changed, when a letter will need another font. */
export function typingHint(missing: string, font: FontInfo): string {
  if (!missing) return "";
  const name = plainFontName(font.name);
  return `${quotedList(missing)} isn’t in this copy of ${name}. Letters will draw ${[...missing].length === 1 ? "it" : "them"} with the closest font it can and tell you which.`;
}

/** the honest line about keeping the font, for the open Edit Text section. */
export function fontPromise(font: FontInfo | null): string {
  if (!font) return "Click a paragraph or line on the page to edit it.";
  const name = plainFontName(font.name);
  if (!font.embedded) return `This text uses ${name} from the reader’s computer, so any letter in its encoding keeps the font.`;
  if (font.subset) return `This text uses part of ${name} stored in the file. Letters it doesn’t hold are drawn in the closest font, and Letters says which.`;
  return `This text uses ${name} stored in the file, so new letters keep the font when it has them.`;
}

/** how an edit is announced to screen readers. */
export function editedAnnouncement(page: number, notices: FontNotice[]): string {
  const changed = `Text changed on page ${page + 1}.`;
  return notices.length > 0 ? `${changed} ${noticeSentence(notices[0])}` : changed;
}

export const ADD_TEXT_DIFFERENCE =
  "Add Text writes real text into the page, as if it had always been there; a text box under Annotate is a comment that floats on top and can be hidden or removed by other readers.";

export const KEEPING_THE_FONT = [
  "Letters keeps the original font for every letter it really holds.",
  "Many PDFs store only the letters a page already uses. A new letter is then copied from the same font on this computer when it is installed and allows it; otherwise the closest standard font (Helvetica, Times or Courier) draws it, and Letters says so beside the text.",
  "Text is re-set from the font’s own widths, so kerning inside the changed words isn’t kept; text after the change keeps its own.",
  "A paragraph that grows doesn’t push the text below it down. Letters says when that happens.",
];

/** the line under the Edit Text button. */
export function editTextEffect(active: boolean): string {
  if (!active) return "Click Edit Text, then a paragraph on the page. Nothing changes until you finish typing.";
  return "Click a paragraph or line to type in it. Click elsewhere or press Ctrl Enter to finish, Escape to cancel. The file on disk changes only when you choose Save.";
}

/** the line under Add Text's button. */
export function addTextEffect(active: boolean, page: number): string {
  if (!active) return `Click Add Text, then where the text should start on the page. Press Escape to stop.`;
  return `Click where the text should start on page ${page + 1}, then type. The file on disk changes only when you choose Save.`;
}

/** the line under the picture tools. */
export function imageEffect(selected: boolean): string {
  if (!selected) return "Click a picture on the page to select it. Drag it to move it; drag a corner to resize it.";
  return "Arrows move the picture, Shift for 10 points; Alt with the arrows resizes it. Delete removes it. Each change is one Undo.";
}
