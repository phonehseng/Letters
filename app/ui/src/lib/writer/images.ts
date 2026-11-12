// pictures in the editor. the bytes stay in the shell; each picture asks
// for its `data:` address once and the answer is kept for the document.
// a cropped picture shows only its kept part, at the whole picture's scale.

import type { Node as PmNode } from "prosemirror-model";
import type { NodeView } from "prosemirror-view";
import { cropOf, cropPlacement, isUncropped } from "./crop";
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
    const free = wrapNote(data) !== null;
    const dom = document.createElement("span");
    // a free picture is placed on the page by the object layer
    // (objectLayer.ts, a node decoration with its left and top) and text
    // flows around it there (wrap.ts)
    dom.className = free ? "w-image w-image-free" : "w-image";
    const width = twipsToPx(Number(data.width ?? 1440));
    const height = twipsToPx(Number(data.height ?? 1440));
    const img = document.createElement("img");
    img.alt = String(data.description ?? "");
    img.draggable = false;
    const crop = cropOf(data);
    if (isUncropped(crop)) {
      img.style.width = `${width}px`;
      img.style.height = `${height}px`;
      dom.append(img);
    } else {
      // the shown part is a window onto the whole picture, at its own scale
      const window = document.createElement("span");
      window.className = "w-crop";
      window.style.width = `${width}px`;
      window.style.height = `${height}px`;
      const whole = cropPlacement({ width, height }, crop);
      Object.assign(img.style, { left: `${whole.left}px`, top: `${whole.top}px`, width: `${whole.width}px`, height: `${whole.height}px` });
      window.append(img);
      dom.append(window);
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