// xml well-formedness: every tag closed, in order, with one root element
// and no attribute written twice. names are compared exactly, as xml is
// case sensitive (unlike html).

import type { Problem } from "../problems";
import { readMarkup, type Tag } from "./markup";
import { positionAt, problemAt } from "./positions";

export const XML_SOURCE = "Letters (XML check)";

export function checkXml(text: string): Problem[] {
  const read = readMarkup(text, false);
  const problems: Problem[] = read.problems.map((problem) => problemAt(text, problem.offset, problem.length, "error", problem.message, XML_SOURCE));
  const stack: Tag[] = [];
  let roots = 0;
  for (const tag of read.tags) {
    problems.push(...repeatedAttributes(text, tag));
    if (tag.kind === "open") {
      if (stack.length === 0 && ++roots === 2) problems.push(at(text, tag, "An XML file has one root element. This is a second one."));
      if (!tag.selfClosing) stack.push(tag);
      continue;
    }
    const open = stack[stack.length - 1];
    if (!open) {
      problems.push(at(text, tag, `</${tag.name}> has no matching <${tag.name}>.`));
    } else if (open.name !== tag.name) {
      const line = positionAt(text, open.offset).line;
      problems.push(at(text, tag, `</${tag.name}> closes <${open.name}> (opened on line ${line}). XML tags must close in the order they opened.`));
      stack.pop();
    } else {
      stack.pop();
    }
  }
  for (const open of stack) problems.push(at(text, open, `<${open.name}> is never closed.`));
  return problems;
}

function at(text: string, tag: Tag, message: string): Problem {
  return problemAt(text, tag.offset, tag.name.length + 1, "error", message, XML_SOURCE);
}

function repeatedAttributes(text: string, tag: Tag): Problem[] {
  const seen = new Set<string>();
  const problems: Problem[] = [];
  for (const attribute of tag.attributes) {
    if (seen.has(attribute.name)) {
      problems.push(problemAt(text, attribute.offset, attribute.name.length, "error", `The attribute “${attribute.name}” is written twice in this tag.`, XML_SOURCE));
    }
    seen.add(attribute.name);
    if (attribute.value === null) {
      problems.push(problemAt(text, attribute.offset, attribute.name.length, "error", `The attribute “${attribute.name}” needs a value in quotes.`, XML_SOURCE));
    }
  }
  return problems;
}
