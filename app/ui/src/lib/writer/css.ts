// how model formatting looks on screen: paragraph properties, run
// properties and marks as css, and one style sheet for the document's
// named styles. pixels are at 100% zoom; the page scales as a whole.

import type { DocumentMeta, ParagraphProps, RunProps, Style } from "./model";
import { defaultParagraphStyle, firstLineOffset, mergeParagraphProps, styleChain, styleRunProps } from "./styles";
import { metricSubstitute } from "./fonts";
import { twipsToPx } from "./units";

const px = (twips: number) => `${Math.round(twipsToPx(twips) * 100) / 100}px`;

/** word's "single" line spacing is a little more than the font size. */
const SINGLE_LINE_HEIGHT = 1.15;

/** css declarations for paragraph properties, as one style attribute. */
export function paragraphCss(props: ParagraphProps | undefined): string {
  if (!props) return "";
  const parts: string[] = [];
  if (props.alignment) parts.push(`text-align:${props.alignment === "justify" ? "justify" : props.alignment}`);
  parts.push(...indentCss(props));
  const spacing = props.spacing;
  if (spacing?.before !== undefined) parts.push(`margin-top:${px(spacing.before)}`);
  if (spacing?.after !== undefined) parts.push(`margin-bottom:${px(spacing.after)}`);
  if (spacing?.line) parts.push(`line-height:${lineHeight(spacing.line.rule, spacing.line.value)}`);
  if (props.pageBreakBefore) parts.push("break-before:page");
  return parts.join(";");
}

/** the indent part alone, which list labels also need. */
export function indentCss(props: ParagraphProps): string[] {
  const indent = props.indent;
  if (!indent) return [];
  const parts: string[] = [];
  if (indent.left !== undefined) parts.push(`margin-left:${px(indent.left)}`);
  if (indent.right !== undefined) parts.push(`margin-right:${px(indent.right)}`);
  if (indent.firstLine !== undefined || indent.hanging !== undefined) parts.push(`text-indent:${px(firstLineOffset(indent))}`);
  return parts;
}

export function lineHeight(rule: string, value: number): string {
  if (rule === "auto") return String(Math.round((value / 240) * SINGLE_LINE_HEIGHT * 1000) / 1000);
  return px(value);
}

/** quotes a font name for css and adds fallbacks of the same kind. */
export function fontStack(name: string): string {
  const serif = /times|georgia|cambria|garamond|book|serif|minion|palatino/i.test(name);
  const safe = name.replace(/["\\]/g, "");
  // a missing office font falls back first to the open font with the same
  // widths (fonts.ts), so lines break where they would in word
  const substitute = metricSubstitute(safe);
  return `"${safe}", ${substitute ? `"${substitute}", ` : ""}${serif ? '"Times New Roman", "Tinos", serif' : '"Calibri", "Carlito", "Segoe UI", sans-serif'}`;
}

/** a highlighter name ("yellow") or six hex digits as a css colour. */
export function highlightColour(value: string): string {
  const named: Record<string, string> = {
    yellow: "#ffff00", green: "#00ff00", cyan: "#00ffff", magenta: "#ff00ff", blue: "#0000ff", red: "#ff0000",
    darkBlue: "#00008b", darkCyan: "#008b8b", darkGreen: "#006400", darkMagenta: "#800080", darkRed: "#8b0000",
    darkYellow: "#808000", darkGray: "#a9a9a9", lightGray: "#d3d3d3", black: "#000000", white: "#ffffff",
  };
  if (named[value]) return named[value];
  return /^[0-9a-f]{6}$/i.test(value) ? `#${value}` : "transparent";
}

export function colour(hex: string): string {
  return /^[0-9a-f]{6}$/i.test(hex) ? `#${hex}` : "inherit";
}

/** css declarations for run properties (a style's character formatting). */
export function runCss(run: RunProps): string {
  const parts: string[] = [];
  if (run.font?.name) parts.push(`font-family:${fontStack(run.font.name)}`);
  if (run.size) parts.push(`font-size:${run.size / 2}pt`);
  if (run.bold !== undefined) parts.push(`font-weight:${run.bold ? 700 : 400}`);
  if (run.italic !== undefined) parts.push(`font-style:${run.italic ? "italic" : "normal"}`);
  const lines: string[] = [];
  if (run.underline && run.underline !== "none") lines.push("underline");
  if (run.strike || run.doubleStrike) lines.push("line-through");
  if (lines.length) parts.push(`text-decoration:${lines.join(" ")}`);
  if (run.smallCaps) parts.push("font-variant:small-caps");
  if (run.allCaps) parts.push("text-transform:uppercase");
  if (run.color && run.color !== "auto") parts.push(`color:${colour(run.color)}`);
  if (run.highlight && run.highlight !== "none") parts.push(`background:${highlightColour(run.highlight)}`);
  if (run.hidden) parts.push("display:none");
  return parts.join(";");
}

/** a css class name for a style id ("Heading 1" has a space in some files). */
export function styleClass(id: string): string {
  return `s-${id.replace(/[^A-Za-z0-9_-]/g, (c) => `_${c.charCodeAt(0).toString(16)}`)}`;
}

function styleRule(meta: DocumentMeta, style: Style, className = styleClass(style.id)): string {
  let paragraph: ParagraphProps = {};
  for (const ancestor of styleChain(meta.styles, style.id).reverse()) paragraph = mergeParagraphProps(paragraph, ancestor.paragraph);
  const declarations = [paragraphCss(paragraph), runCss(styleRunProps(meta.styles, style.id))].filter(Boolean).join(";");
  return `.w-doc .${className}{${declarations}}`;
}

/** paragraphs without a style id use the default style; they carry this class. */
export const DEFAULT_STYLE_CLASS = "s-default";

/**
 * one style sheet for the whole document: the defaults on the page, then
 * a rule per paragraph style. paragraphs carry their style's class, and
 * their own formatting goes on top as an inline style.
 */
export function documentCss(meta: DocumentMeta): string {
  const defaults = meta.styles.defaults;
  // the defaults: run formatting on the page, paragraph formatting on every paragraph
  const rules = [`.w-doc{${runCss(defaults?.run ?? {})}}`, `.w-doc :where(p,h1,h2,h3,h4,h5,h6){${paragraphCss(defaults?.paragraph)}}`];
  for (const style of meta.styles.styles) if (style.kind === "paragraph") rules.push(styleRule(meta, style));
  const fallback = defaultParagraphStyle(meta.styles);
  if (fallback) rules.push(styleRule(meta, fallback, DEFAULT_STYLE_CLASS));
  return rules.join("\n");
}

/** the class a paragraph gets: its style's, or the default style's. */
export function paragraphClass(styleId: string | undefined): string {
  return styleId ? styleClass(styleId) : DEFAULT_STYLE_CLASS;
}
