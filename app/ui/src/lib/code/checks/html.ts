// the html check: tags never closed or closed in the wrong order,
// duplicate ids, pictures without alt text, and the nesting mistakes
// browsers quietly "fix" (a list inside a paragraph, a link in a link).
// it follows html's own rules for void elements (<br>, <img>) and for
// end tags that may be left out (</p>, </li>), so valid pages stay quiet.

import type { Problem } from "../problems";
import { readMarkup, type Tag } from "./markup";
import { positionAt, problemAt } from "./positions";

export const HTML_SOURCE = "Letters (HTML check)";

const VOID_ELEMENTS = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr", "param", "keygen"]);

// html lets these end tags be left out; the next sibling or the parent's
// end closes them
const OPTIONAL_END = new Set(["p", "li", "dt", "dd", "option", "optgroup", "tr", "td", "th", "thead", "tbody", "tfoot", "colgroup", "rt", "rp", "html", "head", "body"]);

// a paragraph ends where one of these starts, so "<p><div>" is never a div in a p
const ENDS_PARAGRAPH = new Set(["address", "article", "aside", "blockquote", "details", "div", "dl", "fieldset", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6", "header", "hr", "main", "nav", "ol", "p", "pre", "section", "table", "ul"]);

const LIST_PARENTS = new Set(["ul", "ol", "menu"]);

interface OpenElement {
  name: string;
  tag: Tag;
}

export function checkHtml(text: string): Problem[] {
  const read = readMarkup(text, true);
  const problems: Problem[] = read.problems.map((problem) => problemAt(text, problem.offset, problem.length, "error", problem.message, HTML_SOURCE));
  const stack: OpenElement[] = [];
  for (const tag of read.tags) {
    const name = tag.name.toLowerCase();
    if (tag.kind === "open") openElement(text, tag, name, stack, problems);
    else closeElement(text, tag, name, stack, problems);
  }
  for (const open of stack) {
    if (!OPTIONAL_END.has(open.name)) problems.push(tagProblem(text, open.tag, "error", `<${open.name}> is never closed.`));
  }
  problems.push(...duplicateIds(text, read.tags), ...missingAltText(text, read.tags));
  return problems;
}

function tagProblem(text: string, tag: Tag, severity: Problem["severity"], message: string): Problem {
  return problemAt(text, tag.offset, tag.name.length + 1, severity, message, HTML_SOURCE);
}

function openElement(text: string, tag: Tag, name: string, stack: OpenElement[], problems: Problem[]) {
  const current = stack[stack.length - 1]?.name;
  if (current === "p" && ENDS_PARAGRAPH.has(name)) {
    if (name !== "p") problems.push(tagProblem(text, tag, "warning", `<${name}> can’t go inside <p>. The browser ends the paragraph before it, so a later </p> will be left over.`));
    stack.pop();
  }
  if ((name === "li" && current === "li") || ((name === "dt" || name === "dd") && (current === "dt" || current === "dd"))) stack.pop();
  if ((name === "td" || name === "th") && (current === "td" || current === "th")) stack.pop();
  if (name === "tr" && (current === "td" || current === "th")) stack.pop();
  if (name === "tr" && stack[stack.length - 1]?.name === "tr") stack.pop();
  if (name === "option" && current === "option") stack.pop();
  const fragment = currentFragment(stack);
  if (name === "a" && fragment.some((open) => open.name === "a")) problems.push(tagProblem(text, tag, "warning", "A link can’t be inside another link."));
  // directly inside a <template> the list it will be put in isn't known yet;
  // the parent is read again because an <li> before it may have just closed
  const isTemplateTop = stack[stack.length - 1]?.name === "template";
  if (name === "li" && !isTemplateTop && !fragment.some((open) => LIST_PARENTS.has(open.name))) {
    problems.push(tagProblem(text, tag, "warning", "<li> belongs inside <ul> or <ol>."));
  }
  if (VOID_ELEMENTS.has(name) || tag.selfClosing) return;
  stack.push({ name, tag });
}

/**
 * the open elements that count as parents: the content of a <template> is
 * a fragment of its own, put into the page later by a script, so the
 * elements around the template aren't its parents.
 */
function currentFragment(stack: OpenElement[]): OpenElement[] {
  const template = findOpen(stack, "template");
  return stack.slice(template + 1);
}

function closeElement(text: string, tag: Tag, name: string, stack: OpenElement[], problems: Problem[]) {
  if (VOID_ELEMENTS.has(name)) return;
  const index = findOpen(stack, name);
  if (index < 0) {
    problems.push(tagProblem(text, tag, "error", `</${name}> has no matching <${name}>.`));
    return;
  }
  // everything opened after it, that needed its own end tag, closes in the wrong order
  for (const inner of stack.slice(index + 1)) {
    if (OPTIONAL_END.has(inner.name)) continue;
    const opened = positionAt(text, inner.tag.offset);
    problems.push(tagProblem(text, tag, "error", `</${name}> closes <${inner.name}> (opened on line ${opened.line}), which is still open.`));
  }
  stack.length = index;
}

function findOpen(stack: OpenElement[], name: string): number {
  for (let index = stack.length - 1; index >= 0; index--) {
    if (stack[index].name === name) return index;
  }
  return -1;
}

function duplicateIds(text: string, tags: Tag[]): Problem[] {
  const firstLine = new Map<string, number>();
  const problems: Problem[] = [];
  for (const tag of tags) {
    const id = tag.attributes.find((attribute) => attribute.name.toLowerCase() === "id");
    if (!id?.value) continue;
    const line = positionAt(text, id.offset).line;
    const first = firstLine.get(id.value);
    if (first === undefined) firstLine.set(id.value, line);
    else problems.push(problemAt(text, id.offset, 2, "error", `The id "${id.value}" is used more than once (first on line ${first}).`, HTML_SOURCE));
  }
  return problems;
}

function missingAltText(text: string, tags: Tag[]): Problem[] {
  return tags
    .filter((tag) => tag.kind === "open" && tag.name.toLowerCase() === "img")
    .filter((tag) => !tag.attributes.some((attribute) => attribute.name.toLowerCase() === "alt"))
    .map((tag) => tagProblem(text, tag, "warning", "This picture has no alt text."));
}
