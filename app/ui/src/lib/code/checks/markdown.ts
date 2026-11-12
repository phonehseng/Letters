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

interface Heading {
  level: number;
  title: string;
  offset: number;
  /** how much of the line the warning underlines */
  length: number;
}

/** a "## Title" heading on this line, or null. */
function atxHeading(line: Line): Heading | null {
  const heading = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line.text);
  if (!heading) return null;
  return { level: heading[1].length, title: heading[2], offset: line.offset, length: heading[1].length };
}

/** a line that may be part of a paragraph, so a setext underline below it makes a heading. */
function isParagraphText(text: string): boolean {
  if (text.trim() === "") return false;
  // headings, list items, quotes and other underlines are blocks of their own
  return !/^ {0,3}(#{1,6}(\s|$)|[-*+]\s|\d+[.)]\s|>|(=+|-+)\s*$)/.test(text);
}

/**
 * "Title" over a line of "=" (level 1) or "Part" over a line of "-"
 * (level 2), when prose[index] is the underline. the heading's text is the
 * whole paragraph above it. a "---" with a blank line above is a thematic
 * break, not a heading, so it gives null.
 */
function setextHeading(prose: Line[], index: number): Heading | null {
  const underline = /^ {0,3}(=+|-+)\s*$/.exec(prose[index].text);
  if (!underline) return null;
  const paragraph: Line[] = [];
  for (let above = index - 1; above >= 0; above--) {
    const line = prose[above];
    // a code fence between the two lines means they aren't neighbours
    const isNextToBelow = line.offset + line.text.length + 1 === prose[above + 1].offset;
    if (!isNextToBelow || !isParagraphText(line.text)) break;
    paragraph.unshift(line);
  }
  if (paragraph.length === 0) return null;
  const title = paragraph.map((line) => line.text.trim()).join(" ");
  return { level: underline[1][0] === "=" ? 1 : 2, title, offset: paragraph[0].offset, length: paragraph[0].text.trim().length };
}

function headingsOf(prose: Line[]): Heading[] {
  const headings: Heading[] = [];
  prose.forEach((line, index) => {
    const heading = atxHeading(line) ?? setextHeading(prose, index);
    if (heading) headings.push(heading);
  });
  return headings;
}

export function checkMarkdown(text: string): Problem[] {
  const problems: Problem[] = [];
  const prose = proseLines(text, problems);
  const anchors = new Set<string>();
  let previousLevel = 0;
  for (const { level, title, offset, length } of headingsOf(prose)) {
    anchors.add(headingAnchor(title));
    if (previousLevel > 0 && level > previousLevel + 1) {
      problems.push(problemAt(text, offset, length, "warning", `This heading jumps from level ${previousLevel} to level ${level}. Screen readers and outlines expect level ${previousLevel + 1} next.`, MARKDOWN_SOURCE));
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
