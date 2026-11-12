// the problems the checks found, as one list: the Problems panel, the
// squiggles and the status bar all read it. pure, so tested on its own.

export type Severity = "error" | "warning" | "info";

export interface Problem {
  /** 1-based line and column, as compilers and people count */
  line: number;
  column: number;
  /** where the underline ends; the end of the word when unknown */
  endLine?: number;
  endColumn?: number;
  severity: Severity;
  message: string;
  /** who said it: "gcc 16.2", "Letters (HTML check)" */
  source: string;
  /** a plain-English sentence for a beginner, under the raw message */
  hint?: string;
}

const SEVERITY_ORDER: Record<Severity, number> = { error: 0, warning: 1, info: 2 };

/** problems in reading order; at one place errors come first. */
export function sortProblems(problems: Problem[]): Problem[] {
  return [...problems].sort((a, b) => a.line - b.line || a.column - b.column || SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);
}

/**
 * one list from the in-app check and the compiler. when both report the
 * same line, the compiler's message is kept: it knows the language better.
 */
export function mergeProblems(inApp: Problem[], compiler: Problem[]): Problem[] {
  const compilerLines = new Set(compiler.filter((problem) => problem.severity === "error").map((problem) => problem.line));
  const kept = inApp.filter((problem) => !(problem.severity === "error" && compilerLines.has(problem.line)));
  return sortProblems(dedupe([...kept, ...compiler]));
}

function dedupe(problems: Problem[]): Problem[] {
  const seen = new Set<string>();
  return problems.filter((problem) => {
    const key = `${problem.line}:${problem.column}:${problem.severity}:${problem.message}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export interface ProblemCounts {
  errors: number;
  warnings: number;
  infos: number;
}

export function countProblems(problems: Problem[]): ProblemCounts {
  return {
    errors: problems.filter((problem) => problem.severity === "error").length,
    warnings: problems.filter((problem) => problem.severity === "warning").length,
    infos: problems.filter((problem) => problem.severity === "info").length,
  };
}

function plural(count: number, word: string): string {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

/** "2 errors, 1 warning" for the status bar, or "No problems found". */
export function countsSentence(counts: ProblemCounts): string {
  const parts: string[] = [];
  if (counts.errors > 0) parts.push(plural(counts.errors, "error"));
  if (counts.warnings > 0) parts.push(plural(counts.warnings, "warning"));
  if (counts.infos > 0) parts.push(plural(counts.infos, "note"));
  return parts.length > 0 ? parts.join(", ") : "No problems found";
}

/** the next problem after a line (wrapping round), for F8. */
export function nextProblem(problems: Problem[], fromLine: number, backwards = false): Problem | null {
  const sorted = sortProblems(problems);
  if (sorted.length === 0) return null;
  if (backwards) {
    const before = sorted.filter((problem) => problem.line < fromLine);
    return before[before.length - 1] ?? sorted[sorted.length - 1];
  }
  return sorted.find((problem) => problem.line > fromLine) ?? sorted[0];
}

/** what a screen reader hears for one problem in the list. */
export function problemLabel(problem: Problem): string {
  const kind = problem.severity === "error" ? "Error" : problem.severity === "warning" ? "Warning" : "Note";
  return `${kind} on line ${problem.line}, column ${problem.column}: ${problem.message}. From ${problem.source}.`;
}
