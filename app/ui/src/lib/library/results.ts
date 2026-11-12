// the rules behind the Search All Documents screen, with no dom: the
// rows the results become, moving through them with the keyboard, the
// highlighted pieces of a snippet, the filter chips, and every sentence
// the screen shows.

import type { DocKind, DocumentHit, Filters, JobStatus, PassageHit, SearchResults } from "./libraryApi";

/** one row the keyboard can land on: a document, or one of its passages. */
export type ResultRow = { kind: "document"; document: number } | { kind: "passage"; document: number; passage: number };

/** every row in reading order: each document, then its passages. */
export function rowsOf(results: SearchResults | null): ResultRow[] {
  const rows: ResultRow[] = [];
  results?.documents.forEach((hit, document) => {
    rows.push({ kind: "document", document });
    hit.passages.forEach((_, passage) => rows.push({ kind: "passage", document, passage }));
  });
  return rows;
}

/** the row after (or before) `current`, staying inside the list. -1 means none chosen yet. */
export function stepRow(current: number, count: number, direction: 1 | -1): number {
  if (count === 0) return -1;
  if (current < 0) return direction === 1 ? 0 : count - 1;
  return Math.min(count - 1, Math.max(0, current + direction));
}

/** the first row of the next (or previous) document: Page Down and Page Up jump by document. */
export function stepDocument(rows: ResultRow[], current: number, direction: 1 | -1): number {
  if (rows.length === 0) return -1;
  const here = rows[current]?.document ?? (direction === 1 ? -1 : rows.length);
  const target = rows.findIndex((row) => row.kind === "document" && row.document === here + direction);
  return target === -1 ? current : target;
}

/** where a row opens: its document, at a passage (the best one for a document row). */
export function targetOf(results: SearchResults, row: ResultRow): { hit: DocumentHit; passage: PassageHit | null } {
  const hit = results.documents[row.document];
  const passage = row.kind === "passage" ? hit.passages[row.passage] : (hit.passages[0] ?? null);
  return { hit, passage };
}

/** a snippet cut into plain and highlighted pieces, for <mark>. */
export function snippetPieces(snippet: string, highlights: [number, number][]): { text: string; marked: boolean }[] {
  const pieces: { text: string; marked: boolean }[] = [];
  let at = 0;
  const sorted = [...highlights].sort((a, b) => a[0] - b[0]);
  for (const [start, end] of sorted) {
    if (start < at || end > snippet.length || start >= end) continue;
    if (start > at) pieces.push({ text: snippet.slice(at, start), marked: false });
    pieces.push({ text: snippet.slice(start, end), marked: true });
    at = end;
  }
  if (at < snippet.length) pieces.push({ text: snippet.slice(at), marked: false });
  return pieces;
}

/** "Page 12", "Page 12 · Methods", or for word documents "About page 12". */
export function passagePlace(kind: DocKind, passage: PassageHit): string {
  const page = kind === "pdf" ? `Page ${passage.page + 1}` : `About page ${passage.page + 1}`;
  return passage.heading ? `${page} · ${passage.heading}` : page;
}

export const KIND_LABELS: Record<DocKind, string> = { pdf: "PDFs", word: "Word Documents", text: "Text and Markdown" };

export type DateChip = "any" | "week" | "month" | "year";

export const DATE_LABELS: Record<DateChip, string> = { any: "Any Time", week: "Past Week", month: "Past Month", year: "Past Year" };

const DAY = 86_400_000;

/** the "changed after" time a date chip stands for. */
export function modifiedAfter(chip: DateChip, now: number): number | null {
  const days = { any: 0, week: 7, month: 31, year: 366 }[chip];
  return days === 0 ? null : now - days * DAY;
}

/** what the chips add up to. */
export function filtersOf(kinds: DocKind[], date: DateChip, titlesOnly: boolean, folder: string | null, now: number): Filters {
  return { kinds, modifiedAfter: modifiedAfter(date, now), titlesOnly, folder };
}

/** a kind chip pressed: on when off, off when on. */
export function toggleKind(kinds: DocKind[], kind: DocKind): DocKind[] {
  return kinds.includes(kind) ? kinds.filter((k) => k !== kind) : [...kinds, kind];
}

/** the live count under the field, read out by screen readers. */
export function countSentence(results: SearchResults | null, query: string): string {
  if (!query.trim() || !results) return "";
  if (results.totalDocuments === 0) return "No results";
  const documents = results.totalDocuments === 1 ? "1 document" : `${results.totalDocuments} documents`;
  const places = results.totalPassages === 1 ? "1 place" : `${results.totalPassages} places`;
  return `${places} in ${documents}`;
}

/** "Showing results for photosynthesis." when a typo was forgiven. */
export function correctionSentence(results: SearchResults | null): string {
  const used = results?.corrections.map((c) => c.used) ?? [];
  return used.length ? `Showing results for “${used.join(" ")}”.` : "";
}

/** what the index is doing, in a few words: "Indexing 132 of 480 documents". */
export function statusSentence(status: JobStatus | null): string {
  if (!status) return "";
  switch (status.phase) {
    case "starting":
    case "scanning":
      return "Looking for new and changed documents…";
    case "indexing":
      return status.total > 0 ? `Indexing ${status.done + 1 > status.total ? status.total : status.done + 1} of ${status.total} documents` : "Indexing…";
    case "paused":
      return "Indexing is paused.";
    case "idle":
      return status.documents === 1 ? "1 document indexed" : `${status.documents.toLocaleString()} documents indexed`;
  }
}

/** true while the index is still filling: results may be missing some documents. */
export function stillIndexing(status: JobStatus | null): boolean {
  return !!status && (status.phase === "indexing" || status.phase === "scanning" || status.phase === "starting");
}

/** the screen's message when there is nothing to show, with the next step. */
export function emptyState(query: string, results: SearchResults | null, status: JobStatus | null, hasSources: boolean): { title: string; detail: string } | null {
  if (!hasSources && (status?.documents ?? 0) === 0) {
    return { title: "Search Every Document at Once", detail: "Add a folder to search, and Letters reads the documents in it on this computer. Nothing is sent anywhere." };
  }
  if (!query.trim()) {
    return { title: "Search All Documents", detail: "Type a word, a “quoted phrase”, or -word to leave one out." };
  }
  if (results && results.totalDocuments === 0) {
    if (stillIndexing(status)) return { title: "No Results Yet", detail: "Some documents are still being indexed. Try again in a moment." };
    return { title: "No Results", detail: "Check the spelling, try fewer words, or add a folder to search." };
  }
  return null;
}

/** the notes on a document, in words ("Locked: only its name is searched"). */
export function noteSentence(note: DocumentHit["notes"][number]): string {
  switch (note.kind) {
    case "locked":
      return "Locked with a password, so only its name is searched.";
    case "needsTextRecognition":
      return note.pages === 1 ? "1 page is a scan. Recognize its text to search it." : `${note.pages} pages are scans. Recognize their text to search them.`;
    case "unreadable":
      return "This file couldn’t be read, so only its name is searched.";
  }
}

/** a file's folder, for the line under its title. */
export function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : path;
}
