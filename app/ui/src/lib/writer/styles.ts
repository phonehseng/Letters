// styles: following a paragraph's style chain to what really applies, and
// the style menu ("easy headers"). a port of the parts of
// core/office/src/styles.rs and outline.rs the editor needs on every
// keystroke, where a round trip to the shell would be too slow.

import type { DocumentMeta, Indent, ParagraphProps, RunProps, Spacing, Style, StyleSheet } from "./model";

const MAX_CHAIN = 32;

export function findStyle(sheet: StyleSheet, id: string | undefined): Style | undefined {
  return id === undefined ? undefined : sheet.styles.find((style) => style.id === id);
}

/** the style and every style it is based on, nearest first. a circle stops. */
export function styleChain(sheet: StyleSheet, id: string): Style[] {
  const chain: Style[] = [];
  let next: string | undefined = id;
  while (next !== undefined && chain.length < MAX_CHAIN && !chain.some((style) => style.id === next)) {
    const style = findStyle(sheet, next);
    if (!style) break;
    chain.push(style);
    next = style.basedOn;
  }
  return chain;
}

export function defaultParagraphStyle(sheet: StyleSheet): Style | undefined {
  return sheet.styles.find((style) => style.kind === "paragraph" && style.isDefault);
}

/** lays `over` on `base` the way the engine's ParagraphProps::merge does. */
export function mergeParagraphProps(base: ParagraphProps, over: ParagraphProps | undefined): ParagraphProps {
  if (!over) return base;
  const merged: ParagraphProps = { ...base };
  for (const key of ["alignment", "keepWithNext", "keepLinesTogether", "pageBreakBefore", "list", "outlineLevel"] as const) {
    if (over[key] !== undefined) (merged as Record<string, unknown>)[key] = over[key];
  }
  merged.indent = mergeIndent(base.indent ?? {}, over.indent);
  merged.spacing = { ...(base.spacing ?? {}), ...definedOnly(over.spacing ?? {}) } as Spacing;
  if (over.tabs && over.tabs.length > 0) {
    const tabs = [...(base.tabs ?? [])];
    for (const stop of over.tabs) {
      const kept = tabs.filter((existing) => existing.position !== stop.position);
      tabs.length = 0;
      tabs.push(...kept);
      if (stop.kind !== "clear") tabs.push(stop);
    }
    merged.tabs = tabs.sort((a, b) => a.position - b.position);
  }
  return merged;
}

function definedOnly<T extends object>(value: T): Partial<T> {
  return Object.fromEntries(Object.entries(value).filter(([, item]) => item !== undefined)) as Partial<T>;
}

/** first line and hanging are two views of one value: setting either replaces both. */
export function mergeIndent(base: Indent, over: Indent | undefined): Indent {
  if (!over) return base;
  const merged: Indent = { ...base };
  if (over.left !== undefined) merged.left = over.left;
  if (over.right !== undefined) merged.right = over.right;
  if (over.firstLine !== undefined || over.hanging !== undefined) {
    merged.firstLine = over.firstLine;
    merged.hanging = over.hanging;
  }
  return merged;
}

/** positive for a first-line indent, negative for a hanging one. */
export function firstLineOffset(indent: Indent): number {
  if (indent.hanging) return -indent.hanging;
  return indent.firstLine ?? 0;
}

/** what really applies to a paragraph: defaults, styles root first, list level, its own. */
export function resolvedParagraphProps(meta: DocumentMeta, styleId: string | undefined, own: ParagraphProps | undefined): ParagraphProps {
  const sheet = meta.styles;
  let props: ParagraphProps = { ...(sheet.defaults?.paragraph ?? {}) };
  const id = styleId ?? defaultParagraphStyle(sheet)?.id;
  if (id) for (const style of styleChain(sheet, id).reverse()) props = mergeParagraphProps(props, style.paragraph);
  const list = own?.list ?? props.list;
  if (list) {
    const level = listLevel(meta, list.listId, list.level);
    if (level?.paragraph) props = mergeParagraphProps(props, { ...level.paragraph, list: undefined });
  }
  return mergeParagraphProps(props, own);
}

export function listLevel(meta: DocumentMeta, listId: string, level: number) {
  const instance = meta.numbering.instances.find((item) => item.id === listId);
  const definition = instance && meta.numbering.definitions.find((item) => item.id === instance.definitionId);
  return definition?.levels.find((item) => item.level === level);
}

/** run formatting a style chain gives, root first. */
export function styleRunProps(sheet: StyleSheet, id: string | undefined): RunProps {
  let props: RunProps = { ...(sheet.defaults?.run ?? {}) };
  const styleId = id ?? defaultParagraphStyle(sheet)?.id;
  if (!styleId) return props;
  for (const style of styleChain(sheet, styleId).reverse()) props = { ...props, ...definedOnly(style.run ?? {}) };
  return props;
}

// --- headings

function levelFromName(name: string): number | null {
  const match = /^heading\s*(\d)$/i.exec(name.trim());
  const level = match ? Number(match[1]) : NaN;
  return level >= 1 && level <= 6 ? level : null;
}

/** the heading level (1 to 6) a style gives, by outline level or by its name. */
export function styleHeadingLevel(sheet: StyleSheet, id: string): number | null {
  for (const style of styleChain(sheet, id)) {
    const outline = style.paragraph?.outlineLevel;
    if (outline !== undefined) return outline < 6 ? outline + 1 : null;
    const named = levelFromName(style.name);
    if (named !== null) return named;
  }
  return null;
}

/** the id of the style for headings of `level`, and the style sheet with it added when it was missing. */
export function headingStyle(sheet: StyleSheet, level: number): { id: string; sheet: StyleSheet } {
  const named = `heading ${level}`;
  const candidates = sheet.styles.filter((style) => style.kind === "paragraph" && styleHeadingLevel(sheet, style.id) === level);
  const preferred = candidates.find((style) => style.name.toLowerCase() === named) ?? candidates[0];
  if (preferred) return { id: preferred.id, sheet };
  const style = newHeadingStyle(sheet, level);
  return { id: style.id, sheet: { ...sheet, styles: [...sheet.styles, style] } };
}

function uniqueId(sheet: StyleSheet, wanted: string): string {
  let id = wanted;
  while (findStyle(sheet, id)) id += "x";
  return id;
}

// the same quiet scale as the engine: 16, 13, 12, 11 point
function newHeadingStyle(sheet: StyleSheet, level: number): Style {
  const basedOn = defaultParagraphStyle(sheet)?.id;
  return {
    id: uniqueId(sheet, `Heading${level}`),
    name: `heading ${level}`,
    kind: "paragraph",
    ...(basedOn ? { basedOn, next: basedOn } : {}),
    isDefault: false,
    paragraph: { outlineLevel: level - 1, keepWithNext: true, spacing: { before: level === 1 ? 360 : 240, after: 120 } },
    run: { bold: true, size: level === 1 ? 32 : level === 2 ? 26 : level === 3 ? 24 : 22 },
  };
}

/** Title and Quote, made the way word makes them when the document has none. */
function newNamedStyle(sheet: StyleSheet, kind: "title" | "quote"): Style {
  const basedOn = defaultParagraphStyle(sheet)?.id;
  const common = { kind: "paragraph" as const, isDefault: false, ...(basedOn ? { basedOn, next: basedOn } : {}) };
  if (kind === "title") {
    return { ...common, id: uniqueId(sheet, "Title"), name: "Title", paragraph: { spacing: { after: 80 } }, run: { size: 56 } };
  }
  return {
    ...common,
    id: uniqueId(sheet, "Quote"),
    name: "Quote",
    paragraph: { alignment: "center", indent: { left: 864, right: 864 }, spacing: { before: 200, after: 160 } },
    run: { italic: true, color: "404040" },
  };
}

/** the document's Title or Quote style, added if missing. */
export function namedStyle(sheet: StyleSheet, kind: "title" | "quote"): { id: string; sheet: StyleSheet } {
  const name = kind === "title" ? "title" : "quote";
  const existing = sheet.styles.find((style) => style.kind === "paragraph" && (style.name.toLowerCase() === name || style.id.toLowerCase() === name));
  if (existing) return { id: existing.id, sheet };
  const style = newNamedStyle(sheet, kind);
  return { id: style.id, sheet: { ...sheet, styles: [...sheet.styles, style] } };
}

/** one entry of the style menu. */
export interface StyleChoice {
  /** "title", "h1".."h3", "body", "quote", or "style:<id>" for the document's own */
  key: string;
  label: string;
  shortcut?: string;
}

export const STANDARD_STYLES: StyleChoice[] = [
  { key: "title", label: "Title" },
  { key: "h1", label: "Heading 1", shortcut: "Ctrl Alt 1" },
  { key: "h2", label: "Heading 2", shortcut: "Ctrl Alt 2" },
  { key: "h3", label: "Heading 3", shortcut: "Ctrl Alt 3" },
  { key: "body", label: "Body", shortcut: "Ctrl Alt 0" },
  { key: "quote", label: "Quote" },
];

/** names people know for word's built-in style names. */
export function displayName(style: Style): string {
  const level = levelFromName(style.name);
  if (level !== null) return `Heading ${level}`;
  if (style.name === "Normal" || style.isDefault) return "Body";
  return style.name.charAt(0).toUpperCase() + style.name.slice(1);
}

/** the document's own paragraph styles that the standard entries don't already cover. */
export function documentStyleChoices(sheet: StyleSheet): StyleChoice[] {
  const covered = new Set(["title", "quote"]);
  return sheet.styles
    .filter((style) => style.kind === "paragraph" && !style.isDefault && styleHeadingLevel(sheet, style.id) === null)
    .filter((style) => !covered.has(style.name.toLowerCase()) && !(style as { hidden?: boolean }).hidden)
    .map((style) => ({ key: `style:${style.id}`, label: displayName(style) }))
    .sort((a, b) => a.label.localeCompare(b.label));
}

/** which menu entry describes a paragraph. */
export function choiceForParagraph(sheet: StyleSheet, styleId: string | undefined, headingLevel: number | null): string {
  if (headingLevel !== null) return headingLevel <= 3 ? `h${headingLevel}` : `style:${styleId ?? ""}`;
  const style = findStyle(sheet, styleId);
  if (!style || style.isDefault) return "body";
  const name = style.name.toLowerCase();
  if (name === "title") return "title";
  if (name === "quote") return "quote";
  return `style:${style.id}`;
}
