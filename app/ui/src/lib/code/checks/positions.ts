// turning a character offset into the line and column people read, and
// making a problem from it. shared by every in-app check.

import type { Problem, Severity } from "../problems";

export interface Position {
  line: number;
  column: number;
}

/** 1-based line and column of `offset` in `text`. */
export function positionAt(text: string, offset: number): Position {
  let line = 1;
  let lineStart = 0;
  const end = Math.min(offset, text.length);
  for (let index = 0; index < end; index++) {
    if (text.charCodeAt(index) === 10) {
      line++;
      lineStart = index + 1;
    }
  }
  return { line, column: end - lineStart + 1 };
}

/** a problem at `offset`, underlined for `length` characters on its line. */
export function problemAt(text: string, offset: number, length: number, severity: Severity, message: string, source: string): Problem {
  const start = positionAt(text, offset);
  return { line: start.line, column: start.column, endLine: start.line, endColumn: start.column + Math.max(1, length), severity, message, source };
}
