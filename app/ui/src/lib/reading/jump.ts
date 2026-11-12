// the page finder's rules, as in core/search/src/jump.rs, so the Go To
// list can update on every key press without a round trip to the shell.
// both have tests for the same cases; change them together.
//
// readers can type "12", "p 12", "page xii" (the page printed as xii),
// "12/300", "50%", "+5", "ch 3", "#methods", "last", and in a word
// document "w 5000".

export interface JumpEntry {
  title: string;
  /** counted from 0 */
  page: number;
  level: number;
  /** word documents: the block the writer scrolls to */
  block?: number | null;
}

export interface JumpDocument {
  pageCount: number;
  /** counted from 0 */
  currentPage: number;
  /** the printed page numbers, one per page, or empty */
  pageLabels: string[];
  outline: JumpEntry[];
  bookmarks: JumpEntry[];
  lastPosition: number | null;
  /** word documents: [block, words before it], in order */
  wordStarts: [number, number][];
}

export type JumpKind = "page" | "label" | "percent" | "relative" | "outline" | "bookmark" | "lastPosition" | "word";

export interface JumpCandidate {
  kind: JumpKind;
  page: number;
  block: number | null;
  label: string;
}

const MAX_NAME_MATCHES = 8;

/** lower case without accents, as the engine folds words. */
export function fold(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/ß/g, "ss")
    .toLowerCase();
}

export function resolveJump(document: JumpDocument, typed: string): JumpCandidate[] {
  const text = fold(typed.trim());
  const found: JumpCandidate[] = [];
  if (!text || document.pageCount === 0) return found;
  if (text.startsWith("#")) {
    names(document, text.slice(1).trim(), found);
    return dedupe(found);
  }
  keywords(document, text, found);
  percent(document, text, found);
  relative(document, text, found);
  wordOffset(document, text, found);
  pages(document, text, found);
  chapters(document, text, found);
  names(document, text, found);
  return dedupe(found);
}

function candidate(document: JumpDocument, kind: JumpKind, page: number, label: string): JumpCandidate {
  return { kind, page: Math.min(page, document.pageCount - 1), block: null, label };
}

export function pageName(document: JumpDocument, page: number): string {
  const label = document.pageLabels[page];
  if (label && label !== String(page + 1)) return `Page ${page + 1} (printed ${label})`;
  return `Page ${page + 1} of ${document.pageCount}`;
}

function keywords(document: JumpDocument, text: string, found: JumpCandidate[]) {
  const last = document.pageCount - 1;
  if (["last", "last position", "where i left off", "resume", "continue"].includes(text)) {
    if (document.lastPosition !== null) found.push(candidate(document, "lastPosition", document.lastPosition, `Where You Left Off, page ${document.lastPosition + 1}`));
  } else if (["start", "beginning", "first", "first page", "top"].includes(text)) {
    found.push(candidate(document, "page", 0, pageName(document, 0)));
  } else if (["end", "last page", "bottom"].includes(text)) {
    found.push(candidate(document, "page", last, pageName(document, last)));
  }
}

function percent(document: JumpDocument, text: string, found: JumpCandidate[]) {
  if (!text.endsWith("%")) return;
  const share = Number(text.slice(0, -1).trim());
  if (!Number.isFinite(share) || share < 0 || share > 100 || text.slice(0, -1).trim() === "") return;
  const page = Math.min(Math.floor((share / 100) * document.pageCount), document.pageCount - 1);
  found.push(candidate(document, "percent", page, `${share}% Through, page ${page + 1}`));
}

function relative(document: JumpDocument, text: string, found: JumpCandidate[]) {
  const match = /^([+-])\s*(\d+)$/.exec(text);
  if (!match) return;
  const steps = Number(match[2]);
  const sign = match[1] === "+" ? 1 : -1;
  const target = Math.max(0, Math.min(document.pageCount - 1, document.currentPage + sign * steps));
  found.push(candidate(document, "relative", target, `${steps} Pages ${sign > 0 ? "On" : "Back"}, page ${target + 1}`));
}

/** "p 12", "page12", "pg. 12" -> "12". a prefix needs a space or dot after it, or a digit. */
export function stripWordPrefix(text: string, prefixes: string[]): string | null {
  for (const prefix of prefixes) {
    if (!text.startsWith(prefix)) continue;
    const after = text.slice(prefix.length);
    const separated = /^[.\s]/.test(after);
    const rest = after.replace(/^\.+/, "").trim();
    if (rest && (separated || /^\d/.test(rest))) return rest;
  }
  return null;
}

function wordOffset(document: JumpDocument, text: string, found: JumpCandidate[]) {
  if (document.wordStarts.length === 0) return;
  const rest = stripWordPrefix(text, ["words", "word", "w"]);
  if (rest === null || !/^\d+$/.test(rest)) return;
  const word = Number(rest);
  let chosen = document.wordStarts[0];
  for (const start of document.wordStarts) {
    if (start[1] < Math.max(word, 1)) chosen = start;
    else break;
  }
  found.push({ kind: "word", page: 0, block: chosen[0], label: `Word ${word}` });
}

function pages(document: JumpDocument, text: string, found: JumpCandidate[]) {
  if (text.startsWith("+") || text.startsWith("-")) return;
  const rest = stripWordPrefix(text, ["pages", "page", "pg", "pp", "p"]) ?? text;
  const first = rest.split(/[/ ]/)[0].trim();
  const labelPages = document.pageLabels.flatMap((label, page) => (label && fold(label) === first ? [page] : []));
  if (/^\d+$/.test(first)) {
    const number = Number(first);
    for (const page of labelPages.filter((p) => p + 1 !== number)) found.push(candidate(document, "label", page, pageName(document, page)));
    if (number >= 1 && number <= document.pageCount) found.push(candidate(document, "page", number - 1, pageName(document, number - 1)));
    return;
  }
  for (const page of labelPages) found.push(candidate(document, "label", page, pageName(document, page)));
}

function startsWithNumber(title: string, number: string): boolean {
  const words = title.split(/[\s.:]+/).filter(Boolean);
  return words[0] === number || words[1] === number;
}

function entryCandidate(entry: JumpEntry, kind: JumpKind): JumpCandidate {
  return { kind, page: entry.page, block: entry.block ?? null, label: entry.title.trim() };
}

function chapters(document: JumpDocument, text: string, found: JumpCandidate[]) {
  const rest = stripWordPrefix(text, ["chapter", "chap", "ch", "section", "sect", "sec", "part", "pt"]);
  if (rest === null) return;
  for (const entry of document.outline) {
    if (startsWithNumber(fold(entry.title), rest)) found.push(entryCandidate(entry, "outline"));
  }
  if (/^\d+$/.test(rest)) {
    const topLevel = document.outline.filter((e) => e.level === 0);
    const entry = topLevel[Number(rest) - 1];
    if (entry) found.push(entryCandidate(entry, "outline"));
  }
}

function names(document: JumpDocument, text: string, found: JumpCandidate[]) {
  if ([...text].length < 2) return;
  const scored: [number, JumpCandidate][] = [];
  const sources: [JumpEntry, JumpKind][] = [...document.bookmarks.map((e): [JumpEntry, JumpKind] => [e, "bookmark"]), ...document.outline.map((e): [JumpEntry, JumpKind] => [e, "outline"])];
  for (const [entry, kind] of sources) {
    const title = fold(entry.title);
    let rank: number;
    if (title === text) rank = 0;
    else if (title.startsWith(text)) rank = 1;
    else if (title.split(/\s+/).some((word) => word.startsWith(text))) rank = 2;
    else if (title.includes(text)) rank = 3;
    else continue;
    scored.push([rank, entryCandidate(entry, kind)]);
  }
  // a stable sort keeps the document's own order among equals
  scored.sort((a, b) => a[0] - b[0]);
  found.push(...scored.slice(0, MAX_NAME_MATCHES).map(([, c]) => c));
}

function dedupe(found: JumpCandidate[]): JumpCandidate[] {
  const seen = new Set<string>();
  return found.filter((c) => {
    const key = `${c.kind}|${c.page}|${c.block}|${c.label}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
