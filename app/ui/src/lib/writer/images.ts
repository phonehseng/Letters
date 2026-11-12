// pictures in the editor. the bytes stay in the shell; each picture asks
// for its `data:` address once and the answer is kept for the document.
// a floating picture is shown on a line of its own with its wrap noted,
// because the editing surface has no layout for text flowing round it.

import type { Node as PmNode } from "prosemirror-model";
import type { NodeView } from "prosemirror-view";
import { twipsToPx } from "./units";

type Data = Record<string, unknown>;

export type MediaLoader = (mediaId: string) => Promise<string | null>;

/** addresses already known, so a copied picture carries its bytes to other apps (schema.ts). */
export const knownMediaUrls = new Map<string, string>();

/** the data addresses of one document's pictures. */
export class MediaCache {
  private urls = new Map<string, Promise<string | null>>();
  constructor(private load: MediaLoader) {}

  url(mediaId: string): Promise<string | null> {
    let known = this.urls.get(mediaId);
    if (!known) {
      known = this.load(mediaId).then((url) => (url && knownMediaUrls.set(mediaId, url), url)).catch(() => null);
      this.urls.set(mediaId, known);
    }
    return known;
  }

  /** a picture added in this session, whose address is already known. */
  remember(mediaId: string, url: string) {
    knownMediaUrls.set(mediaId, url);
    this.urls.set(mediaId, Promise.resolve(url));
  }
}

const WRAP_WORDS: Record<string, string> = {
  square: "text wraps around its box",
  tight: "text wraps close around it",
  through: "text flows through it",
  topAndBottom: "text above and below",
  behindText: "behind the text",
  inFrontOfText: "in front of the text",
};

/** "Floating picture: text wraps around its box", the note under a floating picture. */
export function wrapNote(data: Data): string | null {
  const placement = data.placement as { type?: string; wrap?: string } | undefined;
  if (placement?.type !== "floating") return null;
  return `Floating picture: ${WRAP_WORDS[placement.wrap ?? "square"] ?? "text wraps around it"}. Word places it exactly.`;
}

export function imageView(cache: MediaCache) {
  return (node: PmNode): NodeView => {
    const data = node.attrs.data as Data;
    const note = wrapNote(data);
    const dom = document.createElement("span");
    dom.className = note ? "w-image w-image-free" : "w-image";
    const img = document.createElement("img");
    img.alt = String(data.description ?? "");
    img.draggable = false;
    img.style.width = `${twipsToPx(Number(data.width ?? 1440))}px`;
    img.style.height = `${twipsToPx(Number(data.height ?? 1440))}px`;
    dom.append(img);
    if (note) {
      // a free picture sits where its offsets put it, from its paragraph's
      // top left (the paragraph is the positioned box); text does not wrap
      // around it on this surface, word places it exactly
      const placement = data.placement as { horizontal?: { offset?: number; align?: string }; vertical?: { offset?: number }; wrap?: string };
      Object.assign(dom.style, freeStyle(placement));
      dom.title = note;
    }
    cache.url(String(data.mediaId ?? "")).then((url) => {
      if (url) img.src = url;
      else img.classList.add("w-image-missing");
    });
    return {
      dom,
      selectNode: () => dom.classList.add("w-selected"),
      deselectNode: () => dom.classList.remove("w-selected"),
    };
  };
}

/** where a free picture is drawn, from its placement. */
export function freeStyle(placement: { horizontal?: { offset?: number; align?: string }; vertical?: { offset?: number }; wrap?: string }): Record<string, string> {
  const style: Record<string, string> = { position: "absolute", top: `${twipsToPx(placement.vertical?.offset ?? 0)}px`, zIndex: placement.wrap === "behindText" ? "0" : "2" };
  const align = placement.horizontal?.align;
  if (align === "right") style.right = "0px";
  else if (align === "center") Object.assign(style, { left: "50%", transform: "translateX(-50%)" });
  else style.left = `${twipsToPx(placement.horizontal?.offset ?? 0)}px`;
  return style;
}