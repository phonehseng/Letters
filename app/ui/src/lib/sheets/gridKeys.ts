// the grid's keyboard, the same keys as excel (windows and mac) and
// google sheets, checked against the table in core/sheets
// (help/shortcuts.rs, compared with microsoft's and google's published
// lists). ctrl on windows and command on a mac are one "mod" key here,
// as both apps treat them for these shortcuts.
//
// this file only says which action a key press means; SheetsScreen does it.

import type { KeyPress } from "../commands";

export type Direction = "up" | "down" | "left" | "right";

export type GridAction =
  | { kind: "move"; direction: Direction; extend: boolean }
  | { kind: "edge"; direction: Direction; extend: boolean }
  | { kind: "rowStart"; extend: boolean }
  | { kind: "sheetStart"; extend: boolean }
  | { kind: "sheetEnd"; extend: boolean }
  | { kind: "page"; direction: Direction; extend: boolean }
  | { kind: "enter"; backwards: boolean }
  | { kind: "tab"; backwards: boolean }
  | { kind: "edit" }
  | { kind: "clear" }
  | { kind: "clearAndEdit" }
  | { kind: "selectAll" }
  | { kind: "selectRow" }
  | { kind: "selectColumn" }
  | { kind: "fill"; direction: "down" | "right" }
  | { kind: "toggle"; style: "bold" | "italic" | "underline" | "strike" }
  | { kind: "numberFormat"; code: string; name: string }
  | { kind: "insertNow"; what: "date" | "time" }
  | { kind: "copy"; cut: boolean }
  | { kind: "autoSum" }
  | { kind: "deleteLines" }
  | { kind: "insertLines" }
  | { kind: "hide"; axis: "rows" | "columns"; hidden: boolean }
  | { kind: "sheet"; step: 1 | -1 }
  | { kind: "addSheet" }
  | { kind: "replace" }
  | { kind: "filter" }
  | { kind: "formatCells" }
  | { kind: "zoom"; step: 1 | -1 }
  | { kind: "escape" };

const ARROWS: Record<string, Direction> = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" };

// ctrl + shift + a digit: excel's number formats (read from `code`,
// because shift turns the key into ! @ # and so on)
const FORMAT_DIGITS: Record<string, { code: string; name: string }> = {
  Backquote: { code: "General", name: "General" },
  Digit1: { code: "#,##0.00", name: "Number" },
  Digit2: { code: "h:mm AM/PM", name: "Time" },
  Digit3: { code: "d-mmm-yy", name: "Date" },
  Digit4: { code: '"$"#,##0.00_);("$"#,##0.00)', name: "Currency" },
  Digit5: { code: "0%", name: "Percent" },
  Digit6: { code: "0.00E+00", name: "Scientific" },
};

const STYLE_KEYS: Record<string, "bold" | "italic" | "underline" | "strike"> = {
  KeyB: "bold",
  KeyI: "italic",
  KeyU: "underline",
  // excel's strikethrough is ctrl 5; google sheets uses alt shift 5
  Digit5: "strike",
};

/** keys with ctrl (or command), shift or not. */
function modKey(press: KeyPress): GridAction | null {
  const shift = !!press.shiftKey;
  const arrow = ARROWS[press.key];
  if (arrow) return { kind: "edge", direction: arrow, extend: shift };
  if (press.key === "Home") return { kind: "sheetStart", extend: shift };
  if (press.key === "End") return { kind: "sheetEnd", extend: shift };
  if (press.key === "PageDown" || press.key === "PageUp") return { kind: "sheet", step: press.key === "PageDown" ? 1 : -1 };
  if (shift && FORMAT_DIGITS[press.code]) return { kind: "numberFormat", ...FORMAT_DIGITS[press.code] };
  if (shift && (press.code === "Semicolon" || press.key === ":")) return { kind: "insertNow", what: "time" };
  if (shift && (press.code === "Equal" || press.key === "+")) return { kind: "insertLines" };
  if (shift && press.code === "KeyL") return { kind: "filter" };
  if (press.code === "Digit9" || press.code === "Digit0") {
    return { kind: "hide", axis: press.code === "Digit9" ? "rows" : "columns", hidden: !shift };
  }
  if (shift) return null;
  if (STYLE_KEYS[press.code]) return { kind: "toggle", style: STYLE_KEYS[press.code] };
  return modLetter(press);
}

function modLetter(press: KeyPress): GridAction | null {
  switch (press.code) {
    case "KeyA":
      return { kind: "selectAll" };
    case "KeyC":
      return { kind: "copy", cut: false };
    case "KeyX":
      return { kind: "copy", cut: true };
    case "KeyD":
      return { kind: "fill", direction: "down" };
    case "KeyR":
      return { kind: "fill", direction: "right" };
    case "KeyH":
      return { kind: "replace" };
    case "Digit1":
      return { kind: "formatCells" };
    case "Semicolon":
      // excel for mac: control ; is the date and command ; the time
      return { kind: "insertNow", what: press.metaKey && !press.ctrlKey ? "time" : "date" };
    case "Minus":
      return { kind: "deleteLines" };
    case "Space":
      return { kind: "selectColumn" };
  }
  return null;
}

/** keys without ctrl or command. */
function plainKey(press: KeyPress): GridAction | null {
  const shift = !!press.shiftKey;
  const arrow = ARROWS[press.key];
  if (press.altKey) return altKey(press, arrow);
  if (arrow) return { kind: "move", direction: arrow, extend: shift };
  switch (press.key) {
    case "Enter":
      return { kind: "enter", backwards: shift };
    case "Tab":
      return { kind: "tab", backwards: shift };
    case "F2":
      return { kind: "edit" };
    case "Delete":
      return { kind: "clear" };
    case "Backspace":
      return { kind: "clearAndEdit" };
    case "Home":
      return { kind: "rowStart", extend: shift };
    case "PageDown":
    case "PageUp":
      return { kind: "page", direction: press.key === "PageDown" ? "down" : "up", extend: shift };
    case "F11":
      return shift ? { kind: "addSheet" } : null;
    case "Escape":
      return { kind: "escape" };
    case " ":
      return shift ? { kind: "selectRow" } : null;
  }
  return null;
}

function altKey(press: KeyPress, arrow: Direction | undefined): GridAction | null {
  // alt = is autosum in excel (windows); google sheets uses alt shift =
  if (press.code === "Equal") return { kind: "autoSum" };
  // alt with page up and down moves a screen sideways in excel
  if (press.key === "PageDown" || press.key === "PageUp") return { kind: "page", direction: press.key === "PageDown" ? "right" : "left", extend: !!press.shiftKey };
  // alt with the arrows changes sheet in google sheets (option on a mac)
  if (arrow === "down" || arrow === "up") return { kind: "sheet", step: arrow === "down" ? 1 : -1 };
  if (press.shiftKey && press.code === "Digit5") return { kind: "toggle", style: "strike" };
  return null;
}

/** the grid action a key press stands for, or null for keys the grid leaves alone. */
export function gridKeyAction(press: KeyPress): GridAction | null {
  // altgr arrives as ctrl alt; it types a character, so the grid leaves it alone
  if (press.altGraph) return null;
  if (press.ctrlKey && press.altKey && !press.metaKey) {
    // ctrl alt = and - zoom in excel
    if (press.code === "Equal") return { kind: "zoom", step: 1 };
    if (press.code === "Minus") return { kind: "zoom", step: -1 };
    return null;
  }
  if (press.ctrlKey || press.metaKey) return modKey(press);
  return plainKey(press);
}

/** a key that starts typing into the active cell: one printable character. */
export function startsTyping(press: KeyPress): boolean {
  return press.key.length === 1 && (!!press.altGraph || (!press.ctrlKey && !press.metaKey));
}
