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

// html's named characters: the latin-1 set (&nbsp; to &yuml;, in code
// order from 160) and the punctuation web pages use most. "Zo&euml;" was
// pasted as it was written
const LATIN_1 =
  "nbsp iexcl cent pound curren yen brvbar sect uml copy ordf laquo not shy reg macr deg plusmn sup2 sup3 acute micro para middot cedil sup1 ordm raquo frac14 frac12 frac34 iquest " +
  "Agrave Aacute Acirc Atilde Auml Aring AElig Ccedil Egrave Eacute Ecirc Euml Igrave Iacute Icirc Iuml ETH Ntilde Ograve Oacute Ocirc Otilde Ouml times Oslash Ugrave Uacute Ucirc Uuml Yacute THORN szlig " +
  "agrave aacute acirc atilde auml aring aelig ccedil egrave eacute ecirc euml igrave iacute icirc iuml eth ntilde ograve oacute ocirc otilde ouml divide oslash ugrave uacute ucirc uuml yacute thorn yuml";
const ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  euro: "€",
  ndash: "–",
  mdash: "—",
  lsquo: "‘",
  rsquo: "’",
  ldquo: "“",
  rdquo: "”",
  hellip: "…",
  bull: "•",
  trade: "™",
  minus: "−",
  ...Object.fromEntries(LATIN_1.split(" ").map((name, index) => [name, String.fromCharCode(160 + index)])),
};

function decodeEntities(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#[0-9]+|[a-z][a-z0-9]*);/gi, (whole, name: string) => {
    if (name[0] === "#") {
      const code = name[1] === "x" || name[1] === "X" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
      return Number.isFinite(code) ? String.fromCodePoint(code) : whole;
    }
    // case matters: &Euml; is Ë and &euml; is ë; &AMP; and the like still work
    return ENTITIES[name] ?? ENTITIES[name.toLowerCase()] ?? whole;
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
  // for each column, how many more rows a cell above still covers: a cell
  // with rowspan keeps its column in the rows below, which leave it empty,
  // as excel and google sheets paste it (the cells used to slide left
  // under the wrong headings)
  const covered: number[] = [];
  for (const row of table[0].matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/gi)) {
    const fields: string[] = [];
    const skipCovered = () => {
      while ((covered[fields.length] ?? 0) > 0) {
        covered[fields.length] -= 1;
        fields.push("");
      }
    };
    for (const cell of row[1].matchAll(/<t([dh])([^>]*)>([\s\S]*?)<\/t\1>/gi)) {
      skipCovered();
      // a merged cell spans columns: the cells it covers are empty
      const across = Number(/colspan\s*=\s*["']?(\d+)/i.exec(cell[2])?.[1] ?? 1);
      const down = Number(/rowspan\s*=\s*["']?(\d+)/i.exec(cell[2])?.[1] ?? 1);
      for (let column = 0; column < Math.max(1, across); column += 1) {
        if (down > 1) covered[fields.length] = down - 1;
        fields.push(column === 0 ? cellText(cell[3]) : "");
      }
    }
    // columns at the end of the row still covered from above
    for (let column = fields.length; column < covered.length; column += 1) {
      if ((covered[column] ?? 0) > 0) {
        while (fields.length < column) fields.push("");
        covered[column] -= 1;
        fields.push("");
      }
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
