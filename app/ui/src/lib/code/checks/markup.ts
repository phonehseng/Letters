// a small tag reader shared by the HTML and XML checks. it finds tags,
// their attributes and where each starts; it does not build a tree. the
// checks walk the tags with a stack of open elements.

export interface Attribute {
  name: string;
  value: string | null;
  offset: number;
}

export interface Tag {
  kind: "open" | "close";
  /** as written; the html check compares lower case */
  name: string;
  attributes: Attribute[];
  /** written as <br/> */
  selfClosing: boolean;
  offset: number;
  length: number;
}

export interface MarkupProblem {
  offset: number;
  length: number;
  message: string;
}

export interface ReadMarkup {
  tags: Tag[];
  problems: MarkupProblem[];
}

/** elements whose content is raw text: a "<" inside a script isn't a tag */
const RAW_TEXT_ELEMENTS = new Set(["script", "style", "textarea", "title"]);

const TAG_START = /<(\/?)([A-Za-z][\w:.-]*)/y;
const ATTRIBUTE = /\s*([^\s"'>/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/y;

export function readMarkup(text: string, html: boolean): ReadMarkup {
  const tags: Tag[] = [];
  const problems: MarkupProblem[] = [];
  let index = 0;
  while (index < text.length) {
    const lessThan = text.indexOf("<", index);
    if (lessThan < 0) break;
    const skipped = skipSpecial(text, lessThan, problems);
    if (skipped !== null) {
      index = skipped;
      continue;
    }
    const tag = readTag(text, lessThan, problems);
    if (!tag) {
      index = lessThan + 1;
      continue;
    }
    tags.push(tag);
    index = tag.offset + tag.length;
    if (html && tag.kind === "open" && !tag.selfClosing && RAW_TEXT_ELEMENTS.has(tag.name.toLowerCase())) {
      index = skipRawText(text, index, tag.name);
    }
  }
  return { tags, problems };
}

/** comments, doctypes, cdata and processing instructions: the offset after them, or null. */
function skipSpecial(text: string, at: number, problems: MarkupProblem[]): number | null {
  const endings: [string, string, string][] = [
    ["<!--", "-->", "This comment is never closed. Add -->."],
    ["<![CDATA[", "]]>", "This CDATA section is never closed. Add ]]>."],
    ["<?", "?>", "This instruction is never closed. Add ?>."],
    ["<!", ">", "This declaration is never closed. Add >."],
  ];
  for (const [start, end, unclosed] of endings) {
    if (!text.startsWith(start, at)) continue;
    const close = text.indexOf(end, at + start.length);
    if (close < 0) {
      problems.push({ offset: at, length: start.length, message: unclosed });
      return text.length;
    }
    return close + end.length;
  }
  return null;
}

function readTag(text: string, at: number, problems: MarkupProblem[]): Tag | null {
  TAG_START.lastIndex = at;
  const start = TAG_START.exec(text);
  if (!start) return null;
  const attributes: Attribute[] = [];
  let index = TAG_START.lastIndex;
  for (;;) {
    ATTRIBUTE.lastIndex = index;
    const attribute = ATTRIBUTE.exec(text);
    if (!attribute || attribute.index !== index) break;
    const value = attribute[2] ?? attribute[3] ?? attribute[4] ?? null;
    const nameOffset = index + attribute[0].indexOf(attribute[1]);
    attributes.push({ name: attribute[1], value, offset: nameOffset });
    index = ATTRIBUTE.lastIndex;
  }
  const rest = /^\s*(\/?)>/.exec(text.slice(index, index + 200));
  if (!rest) {
    problems.push({ offset: at, length: start[0].length, message: `The tag <${start[2]}> isn’t finished. Add > at its end.` });
    return null;
  }
  return {
    kind: start[1] === "/" ? "close" : "open",
    name: start[2],
    attributes,
    selfClosing: rest[1] === "/",
    offset: at,
    length: index + rest[0].length - at,
  };
}

function skipRawText(text: string, from: number, name: string): number {
  const close = text.toLowerCase().indexOf(`</${name.toLowerCase()}`, from);
  return close < 0 ? text.length : close;
}
