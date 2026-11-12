// the sentences of the Edit tab's round two: the quiet line after a change
// (what moved), the overflow choices, which font drew new letters, the
// signed-file question, and the words of page setup. kept apart from the
// components so they are tested and read as a set (docs/DESIGN.md: short,
// plain, never "oops").

import type { ChangesSummary, FontExtension, LimitReason, OverflowChoice, OverflowReport } from "./contentApi";

function counted(count: number, one: string, many: string): string {
  return count === 1 ? `1 ${one}` : `${count} ${many}`;
}

/** "a", "a and b", "a, b and c". */
export function listed(parts: string[]): string {
  if (parts.length <= 1) return parts.join("");
  return `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

/** the things that moved, as a list: "3 lines and 1 picture". */
function movedThings(summary: ChangesSummary): string[] {
  const things: string[] = [];
  if (summary.textLinesMoved > 0) things.push(counted(summary.textLinesMoved, "line", "lines"));
  if (summary.imagesMoved > 0) things.push(counted(summary.imagesMoved, "picture", "pictures"));
  if (summary.pathsMoved > 0) things.push(counted(summary.pathsMoved, "shape", "shapes"));
  if (summary.annotationsMoved > 0) things.push(counted(summary.annotationsMoved, "comment or link", "comments or links"));
  return things;
}

/**
 * the quiet line after a change: what else moved, and how. empty when
 * nothing besides the edited thing changed. `heightChange` (points) says
 * whether text below moved down or up.
 */
export function summarySentence(summary: ChangesSummary, heightChange = 0): string {
  const sentences: string[] = [];
  const moved = movedThings(summary);
  if (moved.length > 0) {
    const way = heightChange > 0 ? " down" : heightChange < 0 ? " up" : "";
    const lines = Math.abs(heightChange) > 0 ? ` by ${Math.round(Math.abs(heightChange) * 10) / 10} pt` : "";
    sentences.push(`${capital(listed(moved))} moved${way}${lines}.`);
  }
  if (summary.pathsReshaped > 0) sentences.push(`${capital(counted(summary.pathsReshaped, "rule", "rules"))} stretched with the row.`);
  if (summary.tightening) sentences.push(tighteningSentence(summary));
  if (summary.pagesChanged.length > 1) sentences.push(`Pages ${listed(summary.pagesChanged.map((page) => String(page + 1)))} changed.`);
  sentences.push(...summary.notes);
  return sentences.join(" ");
}

function capital(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function tighteningSentence(summary: ChangesSummary): string {
  const tight = summary.tightening!;
  const parts: string[] = [];
  if (tight.wordSpacingScale < 1) parts.push(`word spaces ${Math.round(tight.wordSpacingScale * 100)}%`);
  if (tight.letterSpacingEm < 0) parts.push(`letters ${tight.letterSpacingEm.toFixed(3)} em closer`);
  if (tight.leadingScale < 1) parts.push(`line spacing ${Math.round(tight.leadingScale * 100)}%`);
  return `The paragraph was set a little tighter (${listed(parts)}).`;
}

/** which font drew letters the document's font lacked, in one sentence. */
export function extensionSentence(extension: FontExtension): string {
  if (extension.sentence) return extension.sentence;
  return `Letters added to ${extension.font} came from ${extension.glyphSource}.`;
}

const LIMIT_WORDS: Record<LimitReason, string> = {
  caller: "the bottom of the area",
  obstacle: "what is below it",
  footer: "the footer",
  bottomMargin: "the bottom margin",
};

/** the first line of the overflow sheet. */
export function overflowTitle(report: OverflowReport): string {
  const amount = Math.round(report.amount * 10) / 10;
  return `The new text runs ${amount} pt past ${LIMIT_WORDS[report.limitReason]}.`;
}

export const OVERFLOW_LABELS: Record<Exclude<OverflowChoice, "ask">, string> = {
  allowIntoMargin: "Run into the Margin",
  tighten: "Tighten",
  carryToNextPage: "Carry to Next Page",
  leave: "Leave It",
};

/** the choices the sheet shows, in the engine's order, without "ask". */
export function overflowChoices(report: OverflowReport) {
  return report.choices
    .filter((option) => option.choice !== "ask")
    .map((option) => ({
      choice: option.choice as Exclude<OverflowChoice, "ask">,
      label: OVERFLOW_LABELS[option.choice as Exclude<OverflowChoice, "ask">],
      sentence: option.detail,
      possible: option.possible,
    }));
}

/** the question before changing a signed file: the engine's sentence names the signer. */
export function consentQuestion(message: string | null, signer: string | null): string {
  if (message) return message;
  const by = signer ? ` by ${signer}` : "";
  return `This PDF is digitally signed${by}. Changing its content will make the signature invalid. Confirm to edit it anyway.`;
}
