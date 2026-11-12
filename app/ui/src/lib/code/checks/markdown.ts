// a few sanity checks for markdown: headings that skip a level, links
// with no address, links to a "#section" no heading makes, and a code
// fence that is never closed. markdown itself can't be "wrong", so these
// are warnings about what a reader would trip over.

import type { Problem } from "../problems";
import { problemAt } from "./positions";

export const MARKDOWN_SOURCE = "Letters (Markdown check)";

/** github's rule for a heading's anchor: lower case, spaces to dashes, punctuation gone. */
export function headingAnchor(title: string): string {
  return title
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .replace(/\s/g, "-");
}

interface Line {
  text: string;
  offset: number;
}

function linesOf(text: string): Line[] {
  const lines: Line[] = [];
  let offset = 0;
  for (const line of text.split("\n")) {
    lines.push({ text: line, offset });
    offset += line.length + 1;
  }
  return lines;
}

/** the lines outside code fences, and a problem for a fence left open. */
function proseLines(text: string, problems: Problem[]): Line[] {
  const prose: Line[] = [];
  let fence: Line | null = null;
  for (const line of linesOf(text)) {
    if (/^\s*(```|~~~)/.test(line.text)) {
      fence = fence ? null : line;
      continue;
    }
    if (!fence) prose.push(line);
  }
  if (fence) problems.push(problemAt(text, fence.offset, 3, "warning", "This code block is never closed. Add ``` on a line of its own where it ends.", MARKDOWN_SOURCE));
  return prose;
}

export function checkMarkdown(text: string): Problem[] {
  const problems: Problem[] = [];
  const prose = proseLines(text, problems);
  const anchors = new Set<string>();
  let previousLevel = 0;
  for (const line of prose) {
    const heading = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line.text);
    if (!heading) continue;
    const level = heading[1].length;
    anchors.add(headingAnchor(heading[2]));
    if (previousLevel > 0 && level > previousLevel + 1) {
      problems.push(problemAt(text, line.offset, level, "warning", `This heading jumps from level ${previousLevel} to level ${level}. Screen readers and outlines expect level ${previousLevel + 1} next.`, MARKDOWN_SOURCE));
    }
    previousLevel = level;
  }
  for (const line of prose) problems.push(...linkProblems(text, line, anchors));
  return problems;
}

const LINK = /!?\[([^\]]*)\]\(([^)\s]*)[^)]*\)/g;

function linkProblems(text: string, line: Line, anchors: Set<string>): Problem[] {
  const problems: Problem[] = [];
  for (const match of line.text.matchAll(LINK)) {
    const offset = line.offset + (match.index ?? 0);
    const address = match[2];
    if (address === "") {
      problems.push(problemAt(text, offset, match[0].length, "warning", `The link “${match[1]}” has no address.`, MARKDOWN_SOURCE));
    } else if (address.startsWith("#") && !anchors.has(address.slice(1).toLowerCase())) {
      problems.push(problemAt(text, offset, match[0].length, "warning", `No heading in this file makes the anchor “${address}”.`, MARKDOWN_SOURCE));
    }
  }
  return problems;
}
