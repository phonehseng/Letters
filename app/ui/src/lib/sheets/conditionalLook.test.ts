// tests of what conditional formats change in a cell's drawing.

import { describe, expect, it } from "vitest";
import { barShape, fillOf, showsText, styleOf, textColorOf } from "./conditionalLook";
import type { StyleLook, ViewCell } from "./sheetsApi";

const cell = (extra: Partial<ViewCell> = {}): ViewCell => ({ row: 0, col: 0, text: "42", style: 0, align: "right", ...extra });

const style = (extra: Partial<StyleLook> = {}): StyleLook => ({
  bold: false,
  italic: false,
  underline: false,
  strike: false,
  size: 14,
  font: "Arial",
  wrap: false,
  vertical: "bottom",
  numberFormat: "General",
  borders: [null, null, null, null],
  ...extra,
});

describe("conditional fills and text", () => {
  it("draws a rule's fill and colour over the cell's own", () => {
    const own = cell({ background: "#ffff00", color: "#0000ff" });
    expect(fillOf(own)).toBe("#ffff00");
    expect(textColorOf(own)).toBe("#0000ff");
    const marked = cell({ background: "#ffff00", color: "#0000ff", conditional: { fill: "#FFC7CE", color: "#9C0006" } });
    expect(fillOf(marked)).toBe("#FFC7CE");
    expect(textColorOf(marked)).toBe("#9C0006");
  });

  it("keeps the cell's own where a rule sets nothing", () => {
    const fillOnly = cell({ color: "#0000ff", conditional: { fill: "#C6EFCE" } });
    expect(textColorOf(fillOnly)).toBe("#0000ff");
    expect(fillOf(cell({ conditional: { color: "#ff0000" } }))).toBeUndefined();
  });

  it("puts a rule's bold, italic, underline and strike over the style", () => {
    const styles = [style({ bold: true, underline: true })];
    expect(styleOf(cell(), styles)).toBe(styles[0]);
    const changed = styleOf(cell({ conditional: { bold: false, italic: true, strike: true } }), styles);
    expect(changed).toMatchObject({ bold: false, italic: true, underline: true, strike: true, size: 14, font: "Arial" });
  });

  it("gives an unstyled empty cell excel's plain style with the rule's changes", () => {
    const blank = cell({ text: "", style: 7, conditional: { bold: true } });
    expect(styleOf(blank, [])).toMatchObject({ bold: true, italic: false, vertical: "bottom" });
  });
});

describe("data bars", () => {
  const box = { x: 100, y: 20, width: 104, height: 24 };

  it("grows from the cell's left edge, inside a small margin", () => {
    const shape = barShape({ fraction: 0.5, color: "#638EC6" }, box, 1);
    expect(shape.bar).toEqual({ x: 102, y: 22, width: 50, height: 20 });
    expect(shape.axisX).toBeUndefined();
  });

  it("scales the margin with the zoom and keeps a full bar inside the cell", () => {
    const shape = barShape({ fraction: 1.4, color: "#638EC6" }, box, 2);
    expect(shape.bar).toEqual({ x: 104, y: 24, width: 96, height: 16 });
  });

  it("grows right from the axis for positive numbers and left for negative ones", () => {
    const positive = barShape({ fraction: 0.5, color: "#638EC6", axis: 0.25 }, box, 1);
    expect(positive.axisX).toBe(127);
    expect(positive.bar.x).toBe(127);
    expect(positive.bar.width).toBe(50);
    const negative = barShape({ fraction: 0.25, color: "#FF0000", axis: 0.25, negative: true }, box, 1);
    expect(negative.bar.x).toBe(102);
    expect(negative.bar.width).toBe(25);
  });

  it("can hide the cell's text", () => {
    expect(showsText(cell())).toBe(true);
    expect(showsText(cell({ conditional: { bar: { fraction: 0.3, color: "#638EC6", hideValue: true } } }))).toBe(false);
  });
});
