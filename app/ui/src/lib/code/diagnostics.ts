// reads what a compiler printed into problems for the editor. one reader
// per output format: gcc and clang ("file:line:col: error: message", which
// the python check prints too), javac ("file:line: error:" with a caret
// line for the column) and node --check (a stack-style report). recorded
// samples of each are in diagnostics.test.ts.

import { hintFor } from "./hints";
import type { Problem, Severity } from "./problems";

export type OutputFormat = "gcc" | "gxx" | "clang" | "clangXx" | "python" | "javac" | "node";

export interface CheckerOutput {
  format: OutputFormat;
  output: string;
  /** the name of the checked copy as it appears in the output */
  fileName: string;
  /** "gcc 16.2", shown as each problem's source */
  checker: string;
  /** the editor's language, for the plain-English hints */
  language: string;
}

export function parseCheckerOutput(result: CheckerOutput): Problem[] {
  const problems = readerFor(result.format)(result.output, result.fileName, result.checker);
  return problems.map((problem) => ({ ...problem, hint: hintFor(result.language, problem.message) }));
}

function readerFor(format: OutputFormat): (output: string, fileName: string, checker: string) => Problem[] {
  if (format === "javac") return parseJavac;
  if (format === "node") return parseNode;
  return parseGccStyle;
}

function lines(output: string): string[] {
  return output.replace(/\r\n?/g, "\n").split("\n");
}

function endsWithFile(path: string, fileName: string): boolean {
  const normalized = path.replace(/\\/g, "/");
  return normalized === fileName || normalized.endsWith(`/${fileName}`);
}

function severityOf(word: string): Severity {
  if (word.includes("error")) return "error";
  if (word === "warning") return "warning";
  return "info";
}

// "path:12:5: error: message"; the lazy path stops at the first ":digits:"
// so a windows drive letter ("C:\...") stays part of the path
const GCC_LINE = /^(.*?):(\d+):(?:(\d+):)?\s*(fatal error|error|warning|note):\s*(.*)$/;
const INCLUDED_FROM = /^In file included from (.*?):(\d+)/;

/** gcc, clang and the python check, which prints in the same form. */
export function parseGccStyle(output: string, fileName: string, checker: string): Problem[] {
  const problems: Problem[] = [];
  let includeLine: number | null = null;
  for (const line of lines(output)) {
    const included = line.match(INCLUDED_FROM);
    if (included && endsWithFile(included[1], fileName)) {
      includeLine = Number(included[2]);
      continue;
    }
    const match = line.match(GCC_LINE);
    if (!match) continue;
    const [, path, lineText, columnText, kind, message] = match;
    if (endsWithFile(path, fileName)) {
      includeLine = null;
      problems.push({ line: Number(lineText), column: Number(columnText ?? 1), severity: severityOf(kind), message, source: checker });
    } else if (includeLine !== null && kind !== "note") {
      // a problem inside a header is shown on the #include line that brought it in
      const header = path.split(/[\\/]/).pop();
      problems.push({ line: includeLine, column: 1, severity: severityOf(kind), message: `In ${header}: ${message}`, source: checker });
    }
  }
  return problems.filter((problem) => problem.severity !== "info" || !problem.message.startsWith("each undeclared identifier"));
}

const JAVAC_LINE = /^(.*?):(\d+):\s*(error|warning):\s*(.*)$/;

/**
 * javac gives no column; the line after the source line has a caret under
 * the place, so the caret's position is the column.
 */
export function parseJavac(output: string, fileName: string, checker: string): Problem[] {
  const problems: Problem[] = [];
  const all = lines(output);
  all.forEach((line, index) => {
    const match = line.match(JAVAC_LINE);
    if (!match || !endsWithFile(match[1], fileName)) return;
    const caretLine = all.slice(index + 1, index + 4).find((next) => /^\s*\^\s*$/.test(next));
    const column = caretLine ? caretLine.indexOf("^") + 1 : 1;
    problems.push({ line: Number(match[2]), column, severity: severityOf(match[3]), message: match[4], source: checker });
  });
  return problems;
}

/**
 * node --check prints the file and line, the source line, a caret line,
 * a blank line, then "SyntaxError: message". only syntax errors are
 * possible, and only the first is reported.
 */
export function parseNode(output: string, fileName: string, checker: string): Problem[] {
  const all = lines(output);
  const locationIndex = all.findIndex((line) => {
    const match = line.match(/^(.*):(\d+)$/);
    return match !== null && endsWithFile(match[1], fileName);
  });
  const errorLine = all.find((line) => /^\w*Error: /.test(line));
  if (locationIndex < 0 || !errorLine) return [];
  const lineNumber = Number(all[locationIndex].match(/:(\d+)$/)?.[1] ?? 1);
  const caretLine = all.slice(locationIndex + 1, locationIndex + 4).find((line) => /^\s*\^+\s*$/.test(line));
  const column = caretLine ? caretLine.indexOf("^") + 1 : 1;
  const message = errorLine.replace(/^\w*Error: /, "");
  return [{ line: lineNumber, column, severity: "error", message, source: checker }];
}
