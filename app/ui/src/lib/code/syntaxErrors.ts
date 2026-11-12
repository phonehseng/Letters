// the places the language's grammar couldn't read, taken from the
// highlighter's own syntax tree. used for the languages without a check of
// their own (c, java, rust, typescript...). a grammar recovers from a
// mistake by marking a small "error" node, so the first few of those are a
// fair hint at where something is wrong, but they are only warnings: the
// grammar is simpler than a real compiler.

import { ensureSyntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import type { Problem } from "./problems";

export const GRAMMAR_SOURCE = "Letters (syntax highlighter)";

/** more than this and the rest are usually echoes of the first mistake */
const MAX_GRAMMAR_PROBLEMS = 5;

/** how long the tree may take to finish, so a check never freezes typing */
const TREE_TIMEOUT_MS = 200;

export function grammarProblems(state: EditorState): Problem[] {
  const tree = ensureSyntaxTree(state, state.doc.length, TREE_TIMEOUT_MS);
  if (!tree) return [];
  const problems: Problem[] = [];
  tree.iterate({
    enter: (node) => {
      if (problems.length >= MAX_GRAMMAR_PROBLEMS) return false;
      if (!node.type.isError) return;
      const line = state.doc.lineAt(node.from);
      problems.push({
        line: line.number,
        column: node.from - line.from + 1,
        endLine: line.number,
        endColumn: Math.min(node.to, line.to) - line.from + 1,
        severity: "warning",
        message: "The highlighter couldn’t read the code here. Something may be missing or extra just before it.",
        source: GRAMMAR_SOURCE,
      });
    },
  });
  return problems;
}
