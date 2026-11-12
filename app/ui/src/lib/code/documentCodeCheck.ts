// checks one block of code found in a document with the same checks a
// code file gets: the in-app check, a compiler when one is installed, and
// the characters the document likely swapped in. problems keep the
// block's own line numbers; the sheet says which page or paragraph.

import { checkInApp } from "./checks";
import type { PreparedBlock } from "./codeBlocks";
import { checkerLine, problemsToShow, runCompilerCheck } from "./compilerCheck";
import { languageInfo, type LanguageId } from "./languages";
import { sortProblems, type Problem } from "./problems";

export interface BlockResult {
  problems: Problem[];
  /** "Checked by gcc 16.2", "Basic check only" */
  checkedBy: string;
}

export async function checkBlock(block: PreparedBlock, language: LanguageId): Promise<BlockResult> {
  const inApp = await checkInApp(language, block.repaired);
  const compiler = await runCompilerCheck(language, block.repaired, null).catch(() => null);
  return {
    // the swapped characters are listed whoever checked the code: no compiler mentions them
    problems: sortProblems([...problemsToShow(inApp, compiler), ...block.swaps]),
    checkedBy: checkerLine(language, compiler),
  };
}

/** the file extension an untitled window for the block starts with. */
export function extensionFor(language: LanguageId): string {
  return languageInfo(language).extensions[0] ?? "txt";
}

/** "3 blocks of code found" and where, for the top of the sheet. */
export function foundSentence(count: number): string {
  if (count === 0) return "No code was found. Letters looks for text in a monospaced font (like Courier or Consolas), code styles, and blocks fenced with ```.";
  return count === 1 ? "1 block of code found." : `${count} blocks of code found.`;
}
