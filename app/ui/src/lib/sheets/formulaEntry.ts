// typing a formula: which word autocomplete should look up, putting a
// chosen function in, when a click or an arrow should point at cells
// instead of leaving the cell, and where the references are. the engine
// has the same rules (help::entry); these run on every key press, so they
// stay here without a trip to the shell.

import { cellName } from "./geometry";
import { rangeName } from "./selection";
import type { CellRef, ColouredReference, Range } from "./sheetsApi";

export function isFormula(text: string): boolean {
  return text.startsWith("=");
}

// a reference or a value can start after these, as in help::entry
const STARTS_AN_ARGUMENT = "=(,+-*/^&<>: ";

/** true when a click on a cell (or an arrow) should put a reference in at `caret`. */
export function expectsReference(text: string, caret: number): boolean {
  if (!isFormula(text)) return false;
  const before = text.slice(0, caret).trimEnd();
  if (before.length === 0) return true;
  return STARTS_AN_ARGUMENT.includes(before[before.length - 1]);
}

/** the function name being typed just before the cursor, for autocomplete. */
export function wordBeforeCaret(text: string, caret: number): { start: number; word: string } | null {
  if (!isFormula(text)) return null;
  let start = caret;
  while (start > 1 && /[A-Za-z0-9._]/.test(text[start - 1])) start -= 1;
  const word = text.slice(start, caret);
  // a word starting with a digit is a number, and one after a quote is text
  if (word.length === 0 || /^[0-9.]/.test(word) || insideQuotes(text, start)) return null;
  // a cell reference such as B12 is not a function being typed
  if (/^[A-Za-z]{1,3}[0-9]+$/.test(word)) return null;
  const before = text[start - 1];
  return STARTS_AN_ARGUMENT.includes(before) ? { start, word } : null;
}

function insideQuotes(text: string, at: number): boolean {
  let quotes = 0;
  for (let i = 0; i < at; i += 1) if (text[i] === '"') quotes += 1;
  return quotes % 2 === 1;
}

/** puts "NAME(" in place of the word being typed; returns the new text and cursor. */
export function acceptFunction(text: string, caret: number, name: string): { text: string; caret: number } {
  const typing = wordBeforeCaret(text, caret);
  const start = typing?.start ?? caret;
  // a bracket already right after the word is used rather than doubled
  const after = text.slice(caret);
  const opening = after.startsWith("(") ? "" : "(";
  const next = `${text.slice(0, start)}${name}${opening}${after}`;
  return { text: next, caret: start + name.length + 1 };
}

/** the text a pointed range puts in a formula: "B3", "A1:C5" or "Sheet2!B3". */
export function pointedReference(range: Range, sheetName: string | null): string {
  const name = range.start.row === range.end.row && range.start.col === range.end.col ? cellName(range.start.row, range.start.col) : rangeName(range);
  if (!sheetName) return name;
  const quoted = /^[A-Za-z_][A-Za-z0-9_.]*$/.test(sheetName) ? sheetName : `'${sheetName.replaceAll("'", "''")}'`;
  return `${quoted}!${name}`;
}

/** while pointing, the formula as it was plus the pointed reference at the cursor. */
export function withPointedReference(base: string, caret: number, reference: string): { text: string; caret: number } {
  return { text: base.slice(0, caret) + reference + base.slice(caret), caret: caret + reference.length };
}

/** a reference's cells on the sheet shown, to outline them in its colour; null for other sheets. */
export function referenceRange(reference: ColouredReference, shownSheet: string): Range | null {
  let text = reference.text;
  const bang = text.lastIndexOf("!");
  if (bang >= 0) {
    const sheet = text.slice(0, bang).replace(/^'|'$/g, "").replaceAll("''", "'");
    if (sheet.toLowerCase() !== shownSheet.toLowerCase()) return null;
    text = text.slice(bang + 1);
  }
  const corners = text.replaceAll("$", "").split(":").map(parseCell);
  if (corners.some((corner) => corner === null)) return null;
  const [a, b = a] = corners as CellRef[];
  return {
    start: { row: Math.min(a.row, b.row), col: Math.min(a.col, b.col) },
    end: { row: Math.max(a.row, b.row), col: Math.max(a.col, b.col) },
  };
}

function parseCell(text: string): CellRef | null {
  const match = /^([A-Za-z]{1,3})([0-9]+)$/.exec(text);
  if (!match) return null;
  let col = 0;
  for (const letter of match[1].toUpperCase()) col = col * 26 + (letter.charCodeAt(0) - 64);
  return { row: Number(match[2]) - 1, col: col - 1 };
}

/** today's date or the time now, as typed into a cell (ctrl ; and ctrl shift ;). */
export function nowText(what: "date" | "time", now = new Date()): string {
  if (what === "date") return `${now.getMonth() + 1}/${now.getDate()}/${now.getFullYear()}`;
  const hours = now.getHours() % 12 || 12;
  const minutes = String(now.getMinutes()).padStart(2, "0");
  return `${hours}:${minutes} ${now.getHours() < 12 ? "AM" : "PM"}`;
}
