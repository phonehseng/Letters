// the office document model as the shell sends it (core/office/src/model).
//
// only the parts the editor reads or changes are typed in detail. every
// object may carry more fields than listed here: the converter keeps
// whatever it does not know and gives it back untouched, so saving never
// loses something the editor did not understand.
//
// lengths are twips (1/1440 inch), font sizes half points.

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };

export type Alignment = "left" | "center" | "right" | "justify";

export interface Indent {
  left?: number;
  right?: number;
  firstLine?: number;
  hanging?: number;
}

export interface LineSpacing {
  rule: "auto" | "exact" | "atLeast";
  value: number;
}

export interface Spacing {
  before?: number;
  after?: number;
  line?: LineSpacing;
}

export interface ListRef {
  listId: string;
  level: number;
}

export interface TabStop {
  position: number;
  kind: "left" | "center" | "right" | "decimal" | "bar" | "clear";
  leader?: string;
}

export interface ParagraphProps {
  alignment?: Alignment;
  indent?: Indent;
  spacing?: Spacing;
  keepWithNext?: boolean;
  keepLinesTogether?: boolean;
  pageBreakBefore?: boolean;
  list?: ListRef;
  tabs?: TabStop[];
  outlineLevel?: number;
  preserved?: string[];
}

/** a paragraph's own fields, without its content. */
export interface ParagraphData {
  styleId?: string;
  props?: ParagraphProps;
  attributes?: Json[];
  [other: string]: unknown;
}

export interface Mark {
  type: string;
  [field: string]: unknown;
}

export interface Inline {
  type: string;
  marks?: Mark[];
  [field: string]: unknown;
}

export interface Block {
  type: string;
  [field: string]: unknown;
}

export interface RunProps {
  styleId?: string;
  font?: { name: string; theme?: string };
  bold?: boolean;
  italic?: boolean;
  allCaps?: boolean;
  smallCaps?: boolean;
  strike?: boolean;
  doubleStrike?: boolean;
  hidden?: boolean;
  color?: string;
  size?: number;
  highlight?: string;
  underline?: string;
  script?: "baseline" | "superscript" | "subscript";
  language?: string;
  [other: string]: unknown;
}

export interface Style {
  id: string;
  name: string;
  kind: "paragraph" | "character" | "table" | "numbering";
  basedOn?: string;
  next?: string;
  isDefault?: boolean;
  paragraph?: ParagraphProps;
  run?: RunProps;
  [other: string]: unknown;
}

export interface StyleSheet {
  defaults?: { paragraph?: ParagraphProps; run?: RunProps };
  styles: Style[];
  themeFonts?: { major?: string; minor?: string };
  [other: string]: unknown;
}

export interface ListLevel {
  level: number;
  format: string | { other: string };
  text: string;
  start?: number;
  alignment?: Alignment;
  paragraph?: ParagraphProps;
  run?: RunProps;
  [other: string]: unknown;
}

export interface ListDefinition {
  id: string;
  name?: string;
  levels: ListLevel[];
  [other: string]: unknown;
}

export interface ListInstance {
  id: string;
  definitionId: string;
  overrides?: { level: number; start?: number; preserved?: string }[];
  [other: string]: unknown;
}

export interface Numbering {
  definitions: ListDefinition[];
  instances: ListInstance[];
  [other: string]: unknown;
}

export interface Margins {
  top: number;
  right: number;
  bottom: number;
  left: number;
  header?: number;
  footer?: number;
  gutter?: number;
}

export interface PageSetup {
  width: number;
  height: number;
  orientation?: "portrait" | "landscape";
  margins: Margins;
}

export interface HeaderFooterRefs {
  default?: string;
  first?: string;
  even?: string;
}

export interface Section {
  page: PageSetup;
  columns?: { count: number; space?: number; separator?: boolean };
  start?: string;
  headers?: HeaderFooterRefs;
  footers?: HeaderFooterRefs;
  differentFirstPage?: boolean;
  pageNumberStart?: number;
  [other: string]: unknown;
}

export interface HeaderFooter {
  id: string;
  kind: "header" | "footer";
  blocks: Block[];
}

export interface Note {
  id: string;
  kind: "footnote" | "endnote";
  special?: string;
  blocks: Block[];
}

export interface Comment {
  id: string;
  author: string;
  initials?: string;
  date?: string;
  blocks: Block[];
}

/** everything but the body text: styles, sections, notes... */
export interface DocumentMeta {
  metadata?: { title?: string; author?: string; language?: string; [other: string]: unknown };
  settings?: { evenAndOddHeaders?: boolean; defaultTabStop?: number; trackChanges?: boolean };
  styles: StyleSheet;
  numbering: Numbering;
  sections: Section[];
  headersFooters: HeaderFooter[];
  notes: Note[];
  comments: Comment[];
  [other: string]: unknown;
}

export interface DocumentModel extends DocumentMeta {
  blocks: Block[];
}

/** an empty letter-size document with one paragraph, like Document::new. */
export function emptyModel(): DocumentModel {
  return {
    styles: { styles: [{ id: "Normal", name: "Normal", kind: "paragraph", isDefault: true }] },
    numbering: { definitions: [], instances: [] },
    sections: [{ page: { width: 12240, height: 15840, margins: { top: 1440, right: 1440, bottom: 1440, left: 1440, header: 720, footer: 720 } } }],
    blocks: [{ type: "paragraph", content: [] }],
    headersFooters: [],
    notes: [],
    comments: [],
  };
}
