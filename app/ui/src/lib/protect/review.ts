// the review before redactions are applied: what will be removed, page
// by page and mark by mark, and the warnings in plain words.
//
// the per-mark details come from the engine's preview (the exact text,
// pictures and drawings under each box, the comments and form values
// that go with them). the warnings and the pictures that are only partly
// blanked come from a dry run: a real redaction of a copy, so the review
// can't promise anything the redaction won't do.

import { orderedMarks, type PendingMark } from "./pendingMarks";
import type { RedactionPreview, RedactionReport } from "./protectApi";

/** what one mark will remove. */
export interface ReviewItem {
  markId: string;
  page: number;
  /** the removed text, one line per entry */
  text: string[];
  pictures: number;
  drawings: number;
  comments: string[];
  formValues: string[];
}

export interface ReviewPage {
  page: number;
  items: ReviewItem[];
  /** from the dry run: pictures taken out whole, and partly blanked */
  picturesRemoved: number;
  picturesBlanked: number;
  /** things removed whole because they could not be cut, as the engine says it */
  wholesale: string[];
}

/** a warning, and the phrase to look for when it is about words left elsewhere. */
export interface ReviewWarning {
  text: string;
  phrase: string | null;
}

/** groups the engine's preview by page, with each area beside its mark. */
export function reviewPages(marks: PendingMark[], preview: RedactionPreview, dryRun: RedactionReport | null): ReviewPage[] {
  const ordered = orderedMarks(marks);
  const pages = new Map<number, ReviewPage>();
  preview.areas.forEach((area, index) => {
    const mark = ordered[index];
    if (!mark) return;
    const page = pages.get(area.page) ?? emptyPage(area.page, dryRun);
    page.items.push({
      markId: mark.id,
      page: area.page,
      text: area.texts,
      pictures: area.images,
      drawings: area.paths,
      comments: area.annotations,
      formValues: area.formValues,
    });
    pages.set(area.page, page);
  });
  return [...pages.values()].sort((a, b) => a.page - b.page);
}

function emptyPage(page: number, dryRun: RedactionReport | null): ReviewPage {
  const report = dryRun?.pages.find((each) => each.page === page);
  return {
    page,
    items: [],
    picturesRemoved: report?.imagesRemoved ?? 0,
    picturesBlanked: report?.imagesRewritten ?? 0,
    wholesale: report?.removedWholesale ?? [],
  };
}

/** true when a mark covers nothing at all: worth saying, it may be in the wrong place. */
export function coversNothing(item: ReviewItem): boolean {
  return item.text.length === 0 && item.pictures === 0 && item.drawings === 0 && item.comments.length === 0 && item.formValues.length === 0;
}

/** what one mark removes, in a short line: "2 pictures, 1 drawing". */
export function itemSummary(item: ReviewItem): string {
  const parts: string[] = [];
  if (item.pictures > 0) parts.push(item.pictures === 1 ? "1 picture" : `${item.pictures} pictures`);
  if (item.drawings > 0) parts.push(item.drawings === 1 ? "1 drawing" : `${item.drawings} drawings`);
  if (item.comments.length > 0) parts.push(item.comments.length === 1 ? "1 comment" : `${item.comments.length} comments`);
  if (item.formValues.length > 0) parts.push(item.formValues.length === 1 ? "1 form answer" : `${item.formValues.length} form answers`);
  if (parts.length === 0 && item.text.length === 0) return "Nothing is under this mark.";
  return parts.join(", ");
}

/** how many lines of text, pictures and drawings the whole redaction takes out. */
export function reviewTotals(pages: ReviewPage[]): { lines: number; pictures: number; drawings: number; comments: number; formValues: number } {
  const items = pages.flatMap((page) => page.items);
  return {
    lines: items.reduce((sum, item) => sum + item.text.length, 0),
    pictures: items.reduce((sum, item) => sum + item.pictures, 0),
    drawings: items.reduce((sum, item) => sum + item.drawings, 0),
    comments: items.reduce((sum, item) => sum + item.comments.length, 0),
    formValues: items.reduce((sum, item) => sum + item.formValues.length, 0),
  };
}

function counted(count: number, one: string, many: string): string {
  return count === 1 ? `1 ${one}` : `${count} ${many}`;
}

/** "a, b and c" */
function joined(parts: string[]): string {
  return parts.length <= 1 ? parts.join("") : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

/** the line above the list: "3 lines of text, 1 picture and 1 comment will be removed." */
export function totalsSentence(totals: ReturnType<typeof reviewTotals>): string {
  const parts = [
    totals.lines > 0 ? counted(totals.lines, "line of text", "lines of text") : "",
    totals.pictures > 0 ? counted(totals.pictures, "picture", "pictures") : "",
    totals.drawings > 0 ? counted(totals.drawings, "drawing", "drawings") : "",
    totals.comments > 0 ? counted(totals.comments, "comment", "comments") : "",
    totals.formValues > 0 ? counted(totals.formValues, "form answer", "form answers") : "",
  ].filter(Boolean);
  if (parts.length === 0) return "The marks cover nothing that can be removed. Check that they are in the right places.";
  return `${joined(parts)} will be removed.`;
}

/** what happens to the pictures on a page: "1 picture is removed whole, 1 is partly blanked." */
export function picturesNote(page: ReviewPage): string | null {
  const parts = [
    page.picturesRemoved > 0 ? `${counted(page.picturesRemoved, "picture is", "pictures are")} removed whole` : "",
    page.picturesBlanked > 0
      ? `${page.picturesRemoved > 0 ? counted(page.picturesBlanked, "is", "are") : counted(page.picturesBlanked, "picture is", "pictures are")} partly blanked, so the rest of ${page.picturesBlanked === 1 ? "it stays" : "them stays"}`
      : "",
  ].filter(Boolean);
  return parts.length > 0 ? `${parts.join("; ")}.` : null;
}

// the engine's sentence for words that stay somewhere else
const STILL_APPEARS = /^“(.+)” still appears (.+)\.$/;

/**
 * the warnings to show: the engine's own sentences (other places the
 * same words appear, shared images, attachments), and what redaction
 * never covers, said every time text is removed.
 */
export function reviewWarnings(report: RedactionReport | null, removesText: boolean): ReviewWarning[] {
  const warnings: ReviewWarning[] = (report?.warnings ?? []).map((text) => {
    const match = STILL_APPEARS.exec(text);
    return { text, phrase: match ? match[1] : null };
  });
  if (removesText) {
    warnings.push({
      text: "Fonts stay in the file. A font can show which letters the document used, but not the words or their order.",
      phrase: null,
    });
  }
  return warnings;
}

/** a scanned page loses its whole picture when part of it is redacted; say so plainly. */
export function wholesaleNote(page: ReviewPage): string | null {
  if (page.wholesale.length === 0) return null;
  return `On page ${page.page + 1}: ${page.wholesale.join(" ")}`;
}
