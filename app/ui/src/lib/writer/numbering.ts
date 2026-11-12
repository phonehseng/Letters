// lists: the label each list paragraph shows ("2.", "b.", "iv.", a
// bullet) and making new bullet and number lists. a port of
// core/office/src/numbering.rs and ListDefinition::standard, so labels
// update while typing without asking the shell.

import type { ListDefinition, ListLevel, Numbering } from "./model";

type Counts = (number | null)[];

/** counts list items in document order, as word does. */
export class ListCounter {
  private counts = new Map<string, Counts>();
  private started = new Set<string>();

  constructor(private numbering: Numbering) {}

  /** the label of the next paragraph in `listId` at `level`, or null. */
  next(listId: string, level: number): string | null {
    if (listId === "0") return null;
    const instance = this.numbering.instances.find((item) => item.id === listId);
    const definition = instance && this.numbering.definitions.find((item) => item.id === instance.definitionId);
    if (!instance || !definition) return null;
    const depth = Math.min(Math.max(level, 0), 8);
    const counts = this.counts.get(definition.id) ?? Array<number | null>(9).fill(null);
    this.counts.set(definition.id, counts);
    const overrideStart = instance.overrides?.find((item) => item.level === depth)?.start;
    const levelStart = (at: number) => definition.levels.find((item) => item.level === at)?.start ?? 1;
    const key = `${instance.id}/${depth}`;
    const fresh = !this.started.has(key);
    this.started.add(key);
    const current = counts[depth];
    if (overrideStart !== undefined && fresh) counts[depth] = overrideStart;
    else if (current !== null) counts[depth] = current + 1;
    else counts[depth] = levelStart(depth);
    for (let deeper = depth + 1; deeper < 9; deeper += 1) counts[deeper] = null;
    for (let upper = 0; upper < depth; upper += 1) if (counts[upper] === null) counts[upper] = levelStart(upper);
    const levelDef = definition.levels.find((item) => item.level === depth);
    return levelDef ? renderLabel(levelDef, definition.levels, counts) : null;
  }
}

function formatName(format: ListLevel["format"]): string {
  return typeof format === "string" ? format : "other";
}

function renderLabel(level: ListLevel, levels: ListLevel[], numbers: Counts): string {
  if (formatName(level.format) === "bullet") return level.text;
  return level.text.replace(/%(\d)/g, (_, digit: string) => {
    const index = Math.min(Math.max(Number(digit) - 1, 0), 8);
    const format = levels.find((item) => item.level === index)?.format ?? "decimal";
    return formatNumber(numbers[index] ?? 1, formatName(format));
  });
}

export function formatNumber(value: number, format: string): string {
  switch (format) {
    case "decimalZero":
      return String(value).padStart(2, "0");
    case "lowerLetter":
      return letters(value);
    case "upperLetter":
      return letters(value).toUpperCase();
    case "lowerRoman":
      return roman(value);
    case "upperRoman":
      return roman(value).toUpperCase();
    case "bullet":
    case "none":
      return "";
    default:
      return String(value);
  }
}

function letters(value: number): string {
  if (value <= 0) return "";
  const letter = String.fromCharCode(97 + ((value - 1) % 26));
  return letter.repeat(Math.floor((value - 1) / 26) + 1);
}

function roman(value: number): string {
  if (value <= 0) return "0";
  const table: [number, string][] = [
    [1000, "m"], [900, "cm"], [500, "d"], [400, "cd"], [100, "c"], [90, "xc"],
    [50, "l"], [40, "xl"], [10, "x"], [9, "ix"], [5, "v"], [4, "iv"], [1, "i"],
  ];
  let rest = value;
  let out = "";
  for (const [amount, text] of table) {
    while (rest >= amount) {
      out += text;
      rest -= amount;
    }
  }
  return out;
}

/** a nine level list like the engine's: bullets, or 1. a. i. numbers. */
export function standardDefinition(id: string, numbered: boolean): ListDefinition {
  const bullets = ["•", "◦", "▪"];
  const formats = ["decimal", "lowerLetter", "lowerRoman"];
  const levels: ListLevel[] = Array.from({ length: 9 }, (_, level) => {
    const slot = level % 3;
    return {
      level,
      format: numbered ? formats[slot] : "bullet",
      text: numbered ? `%${level + 1}.` : bullets[slot],
      start: 1,
      alignment: "left",
      paragraph: { indent: { left: 720 * (level + 1), hanging: 360 } },
    };
  });
  return { id, levels };
}

function nextNumericId(ids: string[]): string {
  const numbers = ids.map((id) => Number.parseInt(id, 10)).filter((value) => Number.isFinite(value));
  return String((numbers.length ? Math.max(...numbers) : 0) + 1);
}

/** whether a list instance shows bullets (at its top level). */
export function isBulletList(numbering: Numbering, listId: string): boolean {
  const instance = numbering.instances.find((item) => item.id === listId);
  const definition = instance && numbering.definitions.find((item) => item.id === instance.definitionId);
  const top = definition?.levels.find((item) => item.level === 0);
  return top ? formatName(top.format) === "bullet" : false;
}

/**
 * a list instance of the wanted kind to put paragraphs in: the one used
 * just before (so numbering carries on), or a new one.
 */
export function listFor(numbering: Numbering, kind: "bullet" | "number", nearby: string | null): { listId: string; numbering: Numbering } {
  if (nearby && nearby !== "0" && isBulletList(numbering, nearby) === (kind === "bullet")) return { listId: nearby, numbering };
  const definitionId = nextNumericId(numbering.definitions.map((item) => item.id));
  const listId = nextNumericId(numbering.instances.map((item) => item.id));
  return {
    listId,
    numbering: {
      ...numbering,
      definitions: [...numbering.definitions, standardDefinition(definitionId, kind === "number")],
      instances: [...numbering.instances, { id: listId, definitionId }],
    },
  };
}
