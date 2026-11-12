// the comments list: annotations grouped into threads (a comment and its
// replies), in reading order, with the words each row shows.
//
// a thread starts with any annotation that is not itself a reply and is
// either a note or has a comment written on it (a highlight with a
// remark, say). replies to replies join the same thread, oldest first.

import { plural, timeAgo } from "../format";
import { kindLabel } from "./annotations";
import type { AnnotationInfo } from "./annotateApi";

export interface CommentThread {
  root: AnnotationInfo;
  replies: AnnotationInfo[];
}

function isComment(annotation: AnnotationInfo): boolean {
  return annotation.kind === "note" || (annotation.contents ?? "").trim() !== "";
}

function seconds(iso: string | null): number {
  const parsed = iso ? Date.parse(iso) : NaN;
  return Number.isFinite(parsed) ? parsed / 1000 : 0;
}

/** the annotation a reply finally answers, following chains of replies. */
function threadRootId(annotation: AnnotationInfo, byId: Map<string, AnnotationInfo>): string {
  let current = annotation;
  const seen = new Set<string>();
  while (current.replyTo && byId.has(current.replyTo) && !seen.has(current.id)) {
    seen.add(current.id);
    current = byId.get(current.replyTo)!;
  }
  return current.id;
}

/** every comment thread, by page, then from the top of the page down, then left to right. */
export function commentThreads(annotations: AnnotationInfo[]): CommentThread[] {
  const byId = new Map(annotations.map((annotation) => [annotation.id, annotation]));
  const threads = new Map<string, CommentThread>();
  for (const annotation of annotations) {
    if (!annotation.replyTo && isComment(annotation)) threads.set(annotation.id, { root: annotation, replies: [] });
  }
  for (const annotation of annotations) {
    if (!annotation.replyTo) continue;
    threads.get(threadRootId(annotation, byId))?.replies.push(annotation);
  }
  for (const thread of threads.values()) {
    thread.replies.sort((a, b) => seconds(a.created ?? a.modified) - seconds(b.created ?? b.modified));
  }
  return [...threads.values()].sort(
    (a, b) => a.root.page - b.root.page || a.root.rect.y - b.root.rect.y || a.root.rect.x - b.root.rect.x,
  );
}

/** the small line under a comment: "Sam · 5 minutes ago · page 3". */
export function commentByline(annotation: AnnotationInfo, nowSeconds: number, withPage = true): string {
  const parts = [annotation.author?.trim() || "Unknown author"];
  const when = seconds(annotation.modified ?? annotation.created);
  if (when > 0) parts.push(timeAgo(when, nowSeconds));
  if (withPage) parts.push(`page ${annotation.page + 1}`);
  return parts.join(" · ");
}

/** what a comment row says when the comment itself is empty: "Note", "Highlight". */
export function commentText(annotation: AnnotationInfo): string {
  const text = (annotation.contents ?? "").trim();
  return text || `${kindLabel(annotation)} without a comment`;
}

/** the caption above the list: "3 comments, 2 replies". */
export function commentsSummary(threads: CommentThread[]): string {
  if (threads.length === 0) return "No comments yet";
  const replies = threads.reduce((count, thread) => count + thread.replies.length, 0);
  const comments = plural(threads.length, "comment");
  if (replies === 0) return comments;
  return `${comments}, ${replies === 1 ? "1 reply" : `${replies} replies`}`;
}
