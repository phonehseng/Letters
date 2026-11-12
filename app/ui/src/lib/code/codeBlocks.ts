// code found in a pdf or a word document (the shell finds the blocks, see
// app/shell-core/src/code/document_code.rs): what language it looks like,
// and the characters that copying through a document often swaps in
// (curly quotes, non-breaking spaces, ligatures, dashes). those are flagged
// as likely typos and can be repaired before the code is checked.

import { guessLanguage, type LanguageId } from "./languages";
import type { Problem } from "./problems";

export const DOCUMENT_SOURCE = "Letters (characters from the document)";

export interface FoundCode {
  text: string;
  /** "page 3" or "paragraph 12" */
  location: string;
  page: number | null;
  paragraph: number | null;
  /** "monospaced font", "code style", "fenced block" */
  foundBy: string;
}

interface Swap {
  char: string;
  replacement: string;
  name: string;
}

// what word processors and pdf makers put in place of what code needs
const SWAPS: Swap[] = [
  { char: "“", replacement: '"', name: "a curly opening quote “" },
  { char: "”", replacement: '"', name: "a curly closing quote ”" },
  { char: "‘", replacement: "'", name: "a curly opening apostrophe ‘" },
  { char: "’", replacement: "'", name: "a curly apostrophe ’" },
  { char: " ", replacement: " ", name: "a non-breaking space" },
  { char: "–", replacement: "-", name: "an en dash –" },
  { char: "—", replacement: "-", name: "an em dash —" },
  { char: "−", replacement: "-", name: "a minus sign −" },
  { char: "ﬁ", replacement: "fi", name: "the ligature ﬁ" },
  { char: "ﬂ", replacement: "fl", name: "the ligature ﬂ" },
  { char: "…", replacement: "...", name: "an ellipsis …" },
  { char: "​", replacement: "", name: "an invisible zero-width space" },
];

/** a problem for each character a document likely swapped into the code. */
export function swappedCharacters(text: string): Problem[] {
  const problems: Problem[] = [];
  const lines = text.split("\n");
  lines.forEach((line, index) => {
    for (const swap of SWAPS) {
      const column = line.indexOf(swap.char);
      if (column < 0) continue;
      problems.push({
        line: index + 1,
        column: column + 1,
        endLine: index + 1,
        endColumn: column + 2,
        severity: "warning",
        message: `This line has ${swap.name}. Code needs ${swap.replacement === "" ? "nothing there" : `“${swap.replacement}”`}; documents often change it when code is copied in.`,
        source: DOCUMENT_SOURCE,
      });
    }
  });
  return problems;
}

/** the text with every swapped character put back to what code expects. */
export function repairSwappedCharacters(text: string): string {
  let repaired = text;
  for (const swap of SWAPS) repaired = repaired.split(swap.char).join(swap.replacement);
  return repaired;
}

export interface PreparedBlock extends FoundCode {
  /** the language the text looks like; the person can change it */
  language: LanguageId;
  /** the text with swapped characters repaired, which is what gets checked */
  repaired: string;
  swaps: Problem[];
}

export function prepareBlock(found: FoundCode): PreparedBlock {
  const repaired = repairSwappedCharacters(found.text);
  return { ...found, language: guessLanguage(repaired), repaired, swaps: swappedCharacters(found.text) };
}

/** the honest sentence shown above the results. */
export const DOCUMENT_CODE_CAVEAT =
  "Code copied into a document can lose characters, spaces and line breaks, especially in a PDF. Treat these results as a hint, and check the original file if you have it.";
