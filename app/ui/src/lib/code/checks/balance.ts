// the basic check every language gets without a compiler: brackets that
// don't pair up, strings left open at the end of a line, and for python
// the indentation (tabs mixed with spaces, a block with nothing indented).
// it knows each language's comments and strings, so a "{" inside a string
// or a comment is never counted.

import type { Problem } from "../problems";
import { problemAt } from "./positions";

export const BASIC_SOURCE = "Letters (basic check)";

interface Syntax {
  lineComment: string | null;
  blockComment: [string, string] | null;
  /** quote characters that start a string or a character */
  quotes: string;
  /** python's """ and ''' strings run over several lines */
  tripleQuotes: boolean;
}

const C_LIKE: Syntax = { lineComment: "//", blockComment: ["/*", "*/"], quotes: "\"'", tripleQuotes: false };

const SYNTAX: Record<string, Syntax> = {
  c: C_LIKE,
  cpp: C_LIKE,
  java: C_LIKE,
  javascript: { ...C_LIKE, quotes: "\"'`" },
  typescript: { ...C_LIKE, quotes: "\"'`" },
  // rust's ' is also a lifetime ('a), so only " counts as a string there
  rust: { ...C_LIKE, quotes: '"' },
  python: { lineComment: "#", blockComment: null, quotes: "\"'", tripleQuotes: true },
  shell: { lineComment: "#", blockComment: null, quotes: "\"'", tripleQuotes: false },
  sql: { lineComment: "--", blockComment: ["/*", "*/"], quotes: "\"'", tripleQuotes: false },
};

const PAIRS: Record<string, string> = { ")": "(", "]": "[", "}": "{" };
const NAMES: Record<string, string> = { "(": "parenthesis", "[": "square bracket", "{": "brace" };

interface Opened {
  char: string;
  offset: number;
}

/** true for the languages this check understands. */
export function hasBasicCheck(language: string): boolean {
  return language in SYNTAX;
}

export function checkBalance(text: string, language: string): Problem[] {
  const syntax = SYNTAX[language];
  if (!syntax) return [];
  const problems: Problem[] = [];
  const stack: Opened[] = [];
  let index = 0;
  while (index < text.length) {
    const skipped = skipCommentOrString(text, index, syntax, problems);
    if (skipped !== index) {
      index = skipped;
      continue;
    }
    const char = text[index];
    if (char === "(" || char === "[" || char === "{") stack.push({ char, offset: index });
    else if (char in PAIRS) closeBracket(text, index, stack, problems);
    index++;
  }
  for (const open of stack) {
    problems.push(problemAt(text, open.offset, 1, "error", `This ${NAMES[open.char]} “${open.char}” is never closed.`, BASIC_SOURCE));
  }
  if (language === "python") problems.push(...checkPythonIndentation(text));
  return problems;
}

function closeBracket(text: string, index: number, stack: Opened[], problems: Problem[]) {
  const char = text[index];
  const open = stack[stack.length - 1];
  if (open?.char === PAIRS[char]) {
    stack.pop();
    return;
  }
  const message = open
    ? `This “${char}” doesn’t match the “${open.char}” it closes. A bracket is missing or extra.`
    : `This “${char}” has no opening “${PAIRS[char]}”.`;
  problems.push(problemAt(text, index, 1, "error", message, BASIC_SOURCE));
  // a matching opener further out is taken as meant, so one mistake isn't reported many times
  const matching = findLast(stack, PAIRS[char]);
  if (matching >= 0) stack.length = matching;
}

function findLast(stack: Opened[], char: string): number {
  for (let index = stack.length - 1; index >= 0; index--) {
    if (stack[index].char === char) return index;
  }
  return -1;
}

function startsComment(text: string, index: number, syntax: Syntax): boolean {
  if (syntax.lineComment && text.startsWith(syntax.lineComment, index)) return true;
  return syntax.blockComment !== null && text.startsWith(syntax.blockComment[0], index);
}

/** the offset after a comment or string starting at `index`, or `index` itself. */
function skipCommentOrString(text: string, index: number, syntax: Syntax, problems: Problem[]): number {
  if (syntax.lineComment && text.startsWith(syntax.lineComment, index)) {
    const end = text.indexOf("\n", index);
    return end < 0 ? text.length : end;
  }
  if (syntax.blockComment && text.startsWith(syntax.blockComment[0], index)) {
    const end = text.indexOf(syntax.blockComment[1], index + 2);
    if (end >= 0) return end + syntax.blockComment[1].length;
    problems.push(problemAt(text, index, 2, "error", `This comment is never closed. Add ${syntax.blockComment[1]}.`, BASIC_SOURCE));
    return text.length;
  }
  const quote = text[index];
  // a character literal such as '{' (rust has these too, beside its 'a lifetimes)
  const character = /^'(\\.[^']{0,8}|[^\\'\n])'/.exec(text.slice(index, index + 12));
  if (quote === "'" && character) return index + character[0].length;
  if (!syntax.quotes.includes(quote)) return index;
  if (syntax.tripleQuotes && text.startsWith(quote.repeat(3), index)) {
    const end = text.indexOf(quote.repeat(3), index + 3);
    return end < 0 ? text.length : end + 3;
  }
  return skipString(text, index, quote, problems);
}

function skipString(text: string, start: number, quote: string, problems: Problem[]): number {
  // template strings in javascript may run over lines; other strings end at the line
  const multiline = quote === "`";
  for (let index = start + 1; index < text.length; index++) {
    const char = text[index];
    if (char === "\\") index++;
    else if (char === quote) return index + 1;
    else if (char === "\n" && !multiline) {
      problems.push(problemAt(text, start, 1, "error", `This ${quote === "'" ? "character or string" : "string"} is missing its closing ${quote}.`, BASIC_SOURCE));
      return index;
    }
  }
  problems.push(problemAt(text, start, 1, "error", `This string is missing its closing ${quote}.`, BASIC_SOURCE));
  return text.length;
}

/** a python line that holds code, as the indentation check sees it. */
interface PythonLine {
  offset: number;
  indent: number;
  /** ends in ":" after a block keyword, so the next line must be indented */
  opensBlock: boolean;
  /** ends in a comma, a backslash or an open bracket, so the next line may be indented freely */
  continues: boolean;
}

const BLOCK_KEYWORD = /^(if|elif|else|for|while|def|class|try|except|finally|with|async|match|case)\b/;

/** `code` has its strings and comments blanked already, so only real brackets and colons count. */
function pythonLine(code: string, offset: number, indent: number): PythonLine {
  const opens = (code.match(/[([{]/g) ?? []).length;
  const closes = (code.match(/[)\]}]/g) ?? []).length;
  return {
    offset,
    indent,
    opensBlock: BLOCK_KEYWORD.test(code) && code.endsWith(":"),
    continues: /[,\\([{]$/.test(code) || opens > closes,
  };
}

/** the text with its comments turned into spaces and its strings into "_", and where each one was. */
interface BlankedText {
  code: string;
  /** [start, end) of every comment and string */
  spans: [number, number][];
}

/**
 * blanks comments and strings, keeping newlines so every offset and line
 * number still match. a string becomes "_" rather than spaces because a
 * line holding only a docstring is still a statement in its block.
 */
function blankCommentsAndStrings(text: string, syntax: Syntax): BlankedText {
  // unclosed strings are reported by checkBalance; here they only need skipping
  const ignored: Problem[] = [];
  const spans: [number, number][] = [];
  let code = "";
  let index = 0;
  while (index < text.length) {
    const skipped = skipCommentOrString(text, index, syntax, ignored);
    if (skipped === index) {
      code += text[index];
      index++;
      continue;
    }
    spans.push([index, skipped]);
    const filler = startsComment(text, index, syntax) ? " " : "_";
    code += text.slice(index, skipped).replace(/[^\n]/g, filler);
    index = skipped;
  }
  return { code, spans };
}

/** true when the line starting at `offset` begins inside a string that started on an earlier line. */
function startsInsideString(spans: [number, number][], offset: number): boolean {
  return spans.some(([start, end]) => start < offset && offset < end);
}

/**
 * tabs mixed with spaces, a line ending in ":" with nothing indented under
 * it, and a line indented deeper than the one before for no reason (often
 * the sign of a ":" lost on the line before). the lines inside a """ string
 * are text, not code, so their indent never counts.
 */
export function checkPythonIndentation(text: string): Problem[] {
  const problems: Problem[] = [];
  const blanked = blankCommentsAndStrings(text, SYNTAX.python);
  let offset = 0;
  let previous: PythonLine | null = null;
  for (const line of blanked.code.split("\n")) {
    const lineOffset = offset;
    offset += line.length + 1;
    // python reads a line that starts inside a string as the rest of the line before it
    if (startsInsideString(blanked.spans, lineOffset)) continue;
    const indent = /^[ \t]*/.exec(line)?.[0] ?? "";
    const code = line.trim();
    if (indent.includes(" ") && indent.includes("\t")) {
      problems.push(problemAt(text, lineOffset, indent.length, "error", "This line mixes tabs and spaces in its indent. Use only spaces.", BASIC_SOURCE));
    }
    if (code !== "") {
      const current = pythonLine(code, lineOffset, indent.length);
      if (previous?.opensBlock && current.indent <= previous.indent) {
        problems.push(problemAt(text, previous.offset, 1, "error", "The line after this one should be indented: a line ending in “:” starts a block.", BASIC_SOURCE));
      }
      if (previous && !previous.opensBlock && !previous.continues && current.indent > previous.indent) {
        problems.push(problemAt(text, lineOffset, indent.length, "error", "Unexpected indent. If the line before starts a block (if, for, def…), it needs a “:” at its end.", BASIC_SOURCE));
      }
      previous = current;
    }
  }
  return problems;
}