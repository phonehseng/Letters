// the css check: braces that don't match, declarations without a colon,
// a missing semicolon between two declarations, and property names the
// browser doesn't know (usually a typo: "colour", "widht"). whether a
// property exists is asked of the web view itself (CSS.supports), so the
// list is always the browser's own; tests pass in a list instead.

import type { Problem } from "../problems";
import { problemAt } from "./positions";

export const CSS_SOURCE = "Letters (CSS check)";

export type PropertyKnown = (name: string) => boolean;

/** asks the web view whether a property name exists. */
export const browserKnowsProperty: PropertyKnown = (name) => {
  if (typeof CSS === "undefined" || typeof CSS.supports !== "function") return true;
  // every property accepts "inherit", so this is true exactly for known names
  return CSS.supports(name, "inherit");
};

// rules whose block holds other rules rather than declarations
const GROUP_AT_RULES = /^@(media|supports|layer|container|document|scope|starting-style)\b/i;

/** comments become spaces, so every offset still points at the same place. */
function blankComments(text: string): string {
  return text.replace(/\/\*[\s\S]*?(\*\/|$)/g, (comment) => comment.replace(/[^\n]/g, " "));
}

interface Block {
  kind: "rule" | "group";
  open: number;
}

export function checkCss(text: string, isKnown: PropertyKnown = browserKnowsProperty): Problem[] {
  const source = blankComments(text);
  const problems: Problem[] = [];
  const stack: Block[] = [];
  let segmentStart = 0;
  // a ";" inside brackets belongs to the value, as in url(data:image/png;base64,…)
  let parenDepth = 0;
  for (let index = 0; index < source.length; index++) {
    const char = source[index];
    if (char === '"' || char === "'") {
      index = skipString(source, index);
      continue;
    }
    if (char === "(") parenDepth++;
    else if (char === ")") parenDepth = Math.max(0, parenDepth - 1);
    else if (char === ";" && parenDepth > 0) continue;
    // a brace always ends the value, so a "(" left open can't swallow the rest of the file
    if (char === "{" || char === "}") parenDepth = 0;
    if (char === "{") {
      const prelude = source.slice(segmentStart, index).trim();
      stack.push({ kind: GROUP_AT_RULES.test(prelude) ? "group" : "rule", open: index });
      segmentStart = index + 1;
    } else if (char === ";" || char === "}") {
      if (stack[stack.length - 1]?.kind === "rule") checkDeclaration(text, source, segmentStart, index, isKnown, problems);
      segmentStart = index + 1;
      if (char === "}" && !stack.pop()) problems.push(problemAt(text, index, 1, "error", "This } has no matching {.", CSS_SOURCE));
    }
  }
  for (const block of stack) problems.push(problemAt(text, block.open, 1, "error", "This { is never closed. Add } where the block ends.", CSS_SOURCE));
  return problems;
}

function skipString(source: string, start: number): number {
  const quote = source[start];
  for (let index = start + 1; index < source.length; index++) {
    if (source[index] === "\\") index++;
    else if (source[index] === quote || source[index] === "\n") return index;
  }
  return source.length;
}

/** the first ":" not inside brackets or quotes; the "data:" in url(data:…) is part of a value. */
function colonOutsideBrackets(segment: string): number {
  let depth = 0;
  for (let index = 0; index < segment.length; index++) {
    const char = segment[index];
    if (char === '"' || char === "'") index = skipString(segment, index);
    else if (char === "(") depth++;
    else if (char === ")") depth = Math.max(0, depth - 1);
    else if (char === ":" && depth === 0) return index;
  }
  return -1;
}

/** one "name: value" between two semicolons. */
function checkDeclaration(text: string, source: string, start: number, end: number, isKnown: PropertyKnown, problems: Problem[]) {
  const segment = source.slice(start, end);
  if (segment.trim() === "" || segment.trim().startsWith("@")) return;
  const leading = segment.length - segment.trimStart().length;
  const offset = start + leading;
  const colon = colonOutsideBrackets(segment);
  if (colon < 0) {
    problems.push(problemAt(text, offset, segment.trim().length, "error", `“${segment.trim()}” needs a colon between the property and its value.`, CSS_SOURCE));
    return;
  }
  const name = segment.slice(leading, colon).trim();
  const value = segment.slice(colon + 1);
  // a second "name:" on a later line means the ; before it was forgotten
  const nextDeclaration = /\n\s*([a-z-]+)\s*:/i.exec(value);
  if (nextDeclaration && !/^\s*$/.test(value.slice(0, nextDeclaration.index))) {
    const at = start + colon + 1 + nextDeclaration.index;
    problems.push(problemAt(text, at, 1, "error", `A semicolon is missing before “${nextDeclaration[1]}”.`, CSS_SOURCE));
  }
  if (isUnknownProperty(name, isKnown)) {
    problems.push(problemAt(text, offset, name.length, "warning", `“${name}” isn’t a CSS property. Check the spelling.`, CSS_SOURCE));
  }
}

function isUnknownProperty(name: string, isKnown: PropertyKnown): boolean {
  // custom properties (--accent) and vendor ones (-webkit-…) are never flagged
  if (name.startsWith("--") || name.startsWith("-")) return false;
  if (!/^[a-zA-Z-]+$/.test(name)) return false;
  return !isKnown(name.toLowerCase());
}
