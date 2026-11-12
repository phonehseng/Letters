// the clipboard of the grid. excel, google sheets and libreoffice put a
// copied range there twice: as tab-separated text and as an html table.
// the html keeps cells with line breaks apart reliably and is all a web
// page offers, so it is read first; the text is the fallback. a copy made
// in letters itself is pasted as a copy of cells, so formulas move their
// references as in excel.

import type { Range } from "./sheetsApi";

/** what letters last put on the clipboard, to know a paste of it again. */
export interface CopiedRange {
  sheet: number;
  range: Range;
  text: string;
  cut: boolean;
}

const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };

function decodeEntities(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z]+);/gi, (whole, name: string) => {
    if (name[0] === "#") {
      const code = name[1] === "x" || name[1] === "X" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
    }
    return ENTITIES[name.toLowerCase()] ?? whole;
  });
}

// a line break inside a cell survives the whitespace folding below as this
const LINE_BREAK = "\u0001";

/** the text of one table cell's html, as a browser would show it. */
function cellText(html: string): string {
  const withBreaks = html.replace(/<br[^>]*>/gi, LINE_BREAK);
  const plain = withBreaks.replace(/<style[\s\S]*?<\/style>/gi, "").replace(/<[^>]+>/g, "");
  // html folds runs of spaces and line breaks in the source into one space
  const folded = plain.replace(/\s+/g, " ").trim();
  return decodeEntities(folded).replaceAll(LINE_BREAK, "\n");
}

/** the rows of the first table in clipboard html, or null when there is none. */
export function rowsFromHtml(html: string): string[][] | null {
  const table = /<table[\s\S]*?<\/table>/i.exec(html);
  if (!table) return null;
  const rows: string[][] = [];
  for (const row of table[0].matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/gi)) {
    const fields: string[] = [];
    for (const cell of row[1].matchAll(/<t([dh])([^>]*)>([\s\S]*?)<\/t\1>/gi)) {
      fields.push(cellText(cell[3]));
      // a merged cell spans columns: the cells it covers are empty
      const span = /colspan\s*=\s*["']?(\d+)/i.exec(cell[2]);
      for (let extra = 1; extra < Number(span?.[1] ?? 1); extra += 1) fields.push("");
    }
    rows.push(fields);
  }
  return rows.length > 0 ? rows : null;
}

/** how a copied range repeats over a larger selection, as excel pastes it. */
export function pasteTargets(source: Range, target: Range): { row: number; col: number }[] {
  const height = source.end.row - source.start.row + 1;
  const width = source.end.col - source.start.col + 1;
  const targetHeight = target.end.row - target.start.row + 1;
  const targetWidth = target.end.col - target.start.col + 1;
  // only a selection that the copy fits a whole number of times repeats it
  const timesDown = targetHeight % height === 0 ? targetHeight / height : 1;
  const timesAcross = targetWidth % width === 0 ? targetWidth / width : 1;
  const places: { row: number; col: number }[] = [];
  for (let down = 0; down < timesDown; down += 1) {
    for (let across = 0; across < timesAcross; across += 1) places.push({ row: target.start.row + down * height, col: target.start.col + across * width });
  }
  return places;
}
