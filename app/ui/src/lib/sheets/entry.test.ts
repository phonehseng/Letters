// tests of typing formulas, the clipboard and the cell cache.

import { describe, expect, it } from "vitest";
import { CellCache } from "./cellCache";
import { pasteTargets, rowsFromHtml } from "./clipboard";
import { acceptFunction, expectsReference, pointedReference, referenceRange, withPointedReference, wordBeforeCaret } from "./formulaEntry";
import type { Range, ViewCell } from "./sheetsApi";

const range = (a: [number, number], b: [number, number]): Range => ({ start: { row: a[0], col: a[1] }, end: { row: b[0], col: b[1] } });

describe("typing a formula", () => {
  it("looks up the word being typed, not references, numbers or text", () => {
    expect(wordBeforeCaret("=SU", 3)).toEqual({ start: 1, word: "SU" });
    expect(wordBeforeCaret("=A1+vlo", 7)).toEqual({ start: 4, word: "vlo" });
    expect(wordBeforeCaret("=B12", 4)).toBeNull();
    expect(wordBeforeCaret('="sum', 5)).toBeNull();
    expect(wordBeforeCaret("=1.5", 4)).toBeNull();
    expect(wordBeforeCaret("sum", 3)).toBeNull();
  });

  it("puts the chosen function in with its bracket", () => {
    expect(acceptFunction("=A1+vlo", 7, "VLOOKUP")).toEqual({ text: "=A1+VLOOKUP(", caret: 12 });
    expect(acceptFunction("=su(A1)", 3, "SUM")).toEqual({ text: "=SUM(A1)", caret: 5 });
  });

  it("points at cells only where a reference can go", () => {
    expect(expectsReference("=", 1)).toBe(true);
    expect(expectsReference("=SUM(", 5)).toBe(true);
    expect(expectsReference("=A1+", 4)).toBe(true);
    expect(expectsReference("=A1", 3)).toBe(false);
    expect(expectsReference("hello", 5)).toBe(false);
  });

  it("writes pointed references as excel does", () => {
    expect(pointedReference(range([0, 0], [0, 0]), null)).toBe("A1");
    expect(pointedReference(range([0, 1], [4, 2]), "Sheet2")).toBe("Sheet2!B1:C5");
    expect(pointedReference(range([0, 0], [0, 0]), "My data")).toBe("'My data'!A1");
    expect(withPointedReference("=SUM()", 5, "A1:A3")).toEqual({ text: "=SUM(A1:A3)", caret: 10 });
  });

  it("outlines references on the sheet shown only", () => {
    const own = referenceRange({ start: 1, end: 6, text: "$B$2:C4", colour: "#0000FF" }, "Data");
    expect(own).toEqual(range([1, 1], [3, 2]));
    expect(referenceRange({ start: 1, end: 9, text: "Other!A1", colour: "#FF0000" }, "Data")).toBeNull();
    expect(referenceRange({ start: 1, end: 9, text: "'data'!A1", colour: "#FF0000" }, "Data")).toEqual(range([0, 0], [0, 0]));
  });
});

describe("the clipboard", () => {
  it("reads an excel html table with line breaks, entities and merged cells", () => {
    const html = `<html><body><table><tr><td colspan=2 class=x>Total &amp; tax</td><td>3</td></tr>
      <tr><td>two<br style='mso-data-placement:same-cell'>lines</td><td> a   b </td><td>&#8364;5</td></tr></table></body></html>`;
    expect(rowsFromHtml(html)).toEqual([
      ["Total & tax", "", "3"],
      ["two\nlines", "a b", "€5"],
    ]);
  });

  it("keeps a row-spanning cell's column in the rows below, as excel pastes a web table", () => {
    const html = `<table><tr><th>Country</th><th>City</th><th>People</th></tr>
      <tr><td rowspan="2">France</td><td>Paris</td><td>2.1</td></tr>
      <tr><td>Lyon</td><td>0.5</td></tr>
      <tr><td>Spain</td><td>Madrid</td><td>3.3</td></tr></table>`;
    expect(rowsFromHtml(html)).toEqual([
      ["Country", "City", "People"],
      ["France", "Paris", "2.1"],
      ["", "Lyon", "0.5"],
      ["Spain", "Madrid", "3.3"],
    ]);
    // a spanning cell at the end of a row, and one spanning both ways
    const corner = `<table><tr><td>a</td><td rowspan=2 colspan=2>wide</td></tr><tr><td>b</td></tr><tr><td>c</td><td>d</td><td>e</td></tr></table>`;
    expect(rowsFromHtml(corner)).toEqual([
      ["a", "wide", ""],
      ["b", "", ""],
      ["c", "d", "e"],
    ]);
  });

  it("decodes html's named characters, keeping their case", () => {
    expect(rowsFromHtml("<table><tr><td>Zo&euml; &amp; co</td><td>&Euml;&eacute; &ndash; &euro;5 &frac12;</td></tr></table>")).toEqual([["Zoë & co", "Ëé – €5 ½"]]);
  });

  it("reads a google sheets table and ignores html without one", () => {
    const html = '<google-sheets-html-origin><style>td{}</style><table><tbody><tr style="height:21px"><td>1</td><td>x</td></tr></tbody></table>';
    expect(rowsFromHtml(html)).toEqual([["1", "x"]]);
    expect(rowsFromHtml("<p>just text</p>")).toBeNull();
  });

  it("repeats a copy over a selection it fits a whole number of times", () => {
    expect(pasteTargets(range([0, 0], [0, 0]), range([5, 1], [7, 1]))).toHaveLength(3);
    expect(pasteTargets(range([0, 0], [1, 0]), range([5, 1], [7, 1]))).toEqual([{ row: 5, col: 1 }]);
  });
});

describe("the cell cache", () => {
  it("fetches each block once and keeps stale cells until fresh ones arrive", async () => {
    let fetches = 0;
    let value = "old";
    const cache = new CellCache(async (_sheet, asked) => {
      fetches += 1;
      return [{ row: asked.start.row, col: asked.start.col, text: value, style: 0, align: "left" } as ViewCell];
    }, () => {});
    await cache.want(0, range([0, 0], [10, 5]));
    await cache.want(0, range([0, 0], [10, 5]));
    expect(fetches).toBe(1);
    cache.changed(1);
    value = "new";
    expect(cache.get(0, 0, 0)?.text).toBe("old");
    expect(cache.isCurrent(0, range([0, 0], [0, 0]))).toBe(false);
    await cache.want(0, range([0, 0], [0, 0]));
    expect(cache.get(0, 0, 0)?.text).toBe("new");
  });
});
