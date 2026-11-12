// pictures from the outside and the editor's own in line or free toggle:
// Insert › Picture…, pasted and dropped picture files (through the
// engine's clipboard::add_image), and In Line or Free for pictures the
// engine can't reach (inside tables and text blocks).

import { NodeSelection, TextSelection } from "prosemirror-state";
import * as edit from "./editing";
import { freeOffsets } from "./resize";
import * as api from "./writerApi";
import type { WriterSession } from "./writerSession.svelte";

type Data = Record<string, unknown>;

/** a page's text width when the document says nothing: letter paper with one-inch margins */
const DEFAULT_TEXT_WIDTH = 9360;
/** twips in a css pixel (1440 per inch, 96 pixels per inch) */
const TWIPS_PER_PX = 15;
/** pieces of a file turned into text at a time: larger ones overflow the call stack */
const BASE64_CHUNK = 0x8000;
/** what a picture that can't be read is sized as */
const UNKNOWN_SIZE = { width: 200, height: 150 };

function textWidth(session: WriterSession): number {
  const section = session.meta.sections?.[0];
  if (!section) return DEFAULT_TEXT_WIDTH;
  return section.page.width - section.page.margins.left - section.page.margins.right;
}

/** Insert › Picture…: a file chosen in the open panel, at most as wide as the text. */
export async function insertPicture(session: WriterSession) {
  const path = await api.choosePicture();
  if (!path) return;
  try {
    const { mediaId, url } = await api.addPicture(session.id, path);
    session.media.remember(mediaId, url);
    const size = await pictureSize(url);
    const width = Math.min(size.width * TWIPS_PER_PX, textWidth(session));
    const height = Math.round((width * size.height) / Math.max(1, size.width));
    const name = path.split(/[\\/]/).pop() ?? "Picture";
    session.run((state) => edit.insertImage(state, mediaId, Math.round(width), height, name));
    session.say("Picture added. Add alt text for it in Format, so screen readers can describe it.");
  } catch (problem) {
    session.fail(problem);
  }
}

/** the selected picture's element on screen, and its paragraph's. */
function imageElements(session: WriterSession): { image: HTMLElement; paragraph: HTMLElement } | null {
  const selected = edit.selectedImage(session.editor);
  const dom = selected && (session.view?.nodeDOM(selected.pos) as HTMLElement | null);
  const image = dom?.querySelector("img") ?? null;
  const paragraph = dom?.closest("p, h1, h2, h3, h4, h5, h6, td") as HTMLElement | null;
  return image && paragraph ? { image, paragraph } : null;
}

/**
 * In Line or Free for a picture the engine can't reach (in a table or a
 * text block): Free starts exactly where the picture is, from its
 * paragraph; In Line puts it at the text position nearest its middle.
 * top-level pictures go through the engine (writerObjects.svelte.ts).
 */
export function setImageFreeLocally(session: WriterSession, free: boolean) {
  const view = session.view;
  const selected = edit.selectedImage(session.editor);
  const elements = imageElements(session);
  if (!view || !selected || !elements) {
    session.say("Click a picture first.");
    return;
  }
  const data = selected.node.attrs.data as Data;
  const isFree = (data.placement as { type?: string } | undefined)?.type === "floating";
  if (free === isFree) return;
  const box = elements.image.getBoundingClientRect();
  if (free) {
    const offsets = freeOffsets(box, elements.paragraph.getBoundingClientRect(), session.zoom / TWIPS_PER_PX);
    const horizontal = { relativeTo: "column", offset: offsets.horizontal };
    const vertical = { relativeTo: "paragraph", offset: offsets.vertical };
    const placement = { type: "floating", horizontal, vertical, wrap: "square" };
    session.run((state) => edit.updateImage(state, (current) => ({ ...current, placement })), false);
    session.announce("The picture is free: drag it anywhere on the page.");
    return;
  }
  const target = view.posAtCoords({ left: box.left + box.width / 2, top: box.top + box.height / 2 });
  session.run((state) => {
    const tr = state.tr.delete(selected.pos, selected.pos + selected.node.nodeSize);
    const at = target ? tr.mapping.map(target.pos) : selected.pos;
    const inline = selected.node.type.create({ data: { ...data, placement: { type: "inline" } } }, null, selected.node.marks);
    tr.insert(at, inline);
    return tr.setSelection(NodeSelection.create(tr.doc, at));
  }, false);
  session.announce("The picture is in line with the text.");
}

const isPictureFile = (file: File) => file.type.startsWith("image/") || /\.(png|jpe?g|gif|bmp|webp|svg)$/i.test(file.name);

/**
 * pastes or drops picture files at the cursor (or at `pos`), through the
 * engine: bitmaps become png, the size fits the text width, and a note
 * says when Word couldn't show the format.
 */
export async function insertPictureFiles(session: WriterSession, files: File[], pos?: number) {
  for (const file of files.filter(isPictureFile)) {
    try {
      const base64 = await fileToBase64(file);
      const description = file.name ? file.name.replace(/\.[^.]+$/, "") : null;
      const pasted = await api.pastePicture(session.id, session.model(), base64, description);
      session.media.remember(pasted.mediaId, pasted.url);
      session.run((state) => {
        const tr = pos === undefined ? state.tr : state.tr.setSelection(TextSelection.near(state.doc.resolve(Math.min(pos, state.doc.content.size))));
        // the inline's own fields are the node's data; its type and marks are the node's
        const data = { ...(pasted.inline as Data) };
        delete data.type;
        delete data.marks;
        return tr.replaceSelectionWith(state.schema.nodes.image.create({ data }));
      });
      if (pasted.note) session.say(pasted.note);
      session.announce("Picture added.");
    } catch (problem) {
      session.fail(problem);
    }
  }
}

/** a file's bytes as base64. */
async function fileToBase64(file: Blob): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let start = 0; start < bytes.length; start += BASE64_CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(start, start + BASE64_CHUNK));
  }
  return btoa(binary);
}

/** a picture's size in pixels, read by loading it. */
function pictureSize(url: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve({ width: image.naturalWidth || UNKNOWN_SIZE.width, height: image.naturalHeight || UNKNOWN_SIZE.height });
    image.onerror = () => resolve(UNKNOWN_SIZE);
    image.src = url;
  });
}
