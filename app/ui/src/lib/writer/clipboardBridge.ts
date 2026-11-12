// the system clipboard and the writer, through the office engine.
//
// copying a picture (or a text block) puts html, plain text and, for a
// lone picture, the picture itself as png on the clipboard, so other apps
// get the bytes and not an address they can't reach (engine:
// clipboard::copy_blocks). pasting pictures, picture files and html that
// holds pictures goes through clipboard::add_image and
// import::from_html_with_images. plain text and Letters' own copies are
// left to the editor, which keeps every model detail of what was copied.

import { Fragment, Slice } from "prosemirror-model";
import { NodeSelection } from "prosemirror-state";
import { blocksToNodes } from "./convert";
import type { Block, DocumentMeta, ListDefinition, ListInstance } from "./model";
import { setMeta } from "./editing";
import * as api from "./writerApi";
import type { WriterSession } from "./writerSession.svelte";

type Data = Record<string, unknown>;

/** the model blocks for a selected picture or text block, or null for a text selection. */
export function copiedBlocks(selection: unknown): Block[] | null {
  if (!(selection instanceof NodeSelection)) return null;
  const node = selection.node;
  const data = node.attrs.data as Data;
  if (node.type.name === "image") {
    const marks = node.marks.map((mark) => ({ type: mark.type.name, ...(mark.attrs.data as Data) }));
    const inline = marks.length ? { type: "image", ...data, marks } : { type: "image", ...data };
    return [{ type: "paragraph", content: [inline] }];
  }
  return null;
}

/** copy and cut: pictures get their bytes on the clipboard. true when handled here. */
export function copySelection(session: WriterSession, event: ClipboardEvent, cut: boolean): boolean {
  const view = session.view;
  if (!view) return false;
  const blocks = copiedBlocks(view.state.selection);
  if (!blocks || typeof ClipboardItem === "undefined" || !navigator.clipboard?.write) return false;
  event.preventDefault();
  const answer = api.copyBlocks(session.id, session.model(), blocks);
  const item = new ClipboardItem({
    "text/html": answer.then((copied) => new Blob([copied.html], { type: "text/html" })),
    "text/plain": answer.then((copied) => new Blob([copied.text], { type: "text/plain" })),
    "image/png": answer.then((copied) => pngBlob(copied.image)),
  });
  navigator.clipboard.write([item]).then(
    () => session.announce(cut ? "Picture cut." : "Picture copied."),
    (problem) => session.fail(problem),
  );
  if (cut) session.run((state) => state.tr.deleteSelection());
  return true;
}

/** the system clipboard takes pictures as png only: other formats are drawn once to convert them. */
async function pngBlob(image: [string, string] | null): Promise<Blob> {
  if (!image) return new Blob([], { type: "image/png" });
  const [base64, type] = image;
  const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
  const blob = new Blob([bytes], { type });
  if (type === "image/png") return blob;
  const bitmap = await createImageBitmap(blob);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
  return new Promise((resolve) => canvas.toBlob((png) => resolve(png ?? blob), "image/png"));
}

/** html that should go through the engine: it holds pictures and isn't a copy from this editor. */
export function needsEngineHtml(html: string): boolean {
  return /<img[\s>]/i.test(html) && !html.includes("data-json");
}

/** paste: pictures and html with pictures through the engine. true when handled here. */
export function pasteFromClipboard(session: WriterSession, event: ClipboardEvent): boolean {
  const data = event.clipboardData;
  if (!data) return false;
  const files = [...data.files].filter((file) => file.type.startsWith("image/"));
  if (files.length) {
    session.insertPictureFiles(files);
    return true;
  }
  const html = data.getData("text/html");
  if (!html || !needsEngineHtml(html)) return false;
  api.pasteHtml(session.id, html).then(
    (pasted) => insertPasted(session, pasted),
    (problem) => session.fail(problem),
  );
  return true;
}

function insertPasted(session: WriterSession, pasted: api.PastedHtml) {
  for (const [mediaId, url] of pasted.pictures) session.media.remember(mediaId, url);
  const nodes = blocksToNodes(pasted.blocks as Block[]);
  if (nodes.length === 0) return;
  session.run((state) => {
    // the first and last paragraphs join the text around the cursor, as a paste does
    const openStart = nodes[0].isTextblock ? 1 : 0;
    const openEnd = nodes[nodes.length - 1].isTextblock ? 1 : 0;
    const tr = state.tr.replaceSelection(new Slice(Fragment.from(nodes), openStart, openEnd));
    const meta = state.doc.attrs.meta as DocumentMeta;
    const numbering = mergeNumbering(meta, pasted.numbering);
    if (numbering) setMeta(tr, { ...meta, numbering });
    return tr;
  });
  session.announce(pasted.pictures.length === 1 ? "Pasted with 1 picture." : `Pasted with ${pasted.pictures.length} pictures.`);
}

/** the document's lists plus the pasted ones it doesn't have yet, or null when nothing is new. */
export function mergeNumbering(meta: DocumentMeta, pasted: { definitions?: unknown[]; instances?: unknown[] }): DocumentMeta["numbering"] | null {
  const current = meta.numbering ?? { definitions: [], instances: [] };
  const definitions = ((pasted.definitions ?? []) as ListDefinition[]).filter((one) => !current.definitions.some((each) => each.id === one.id));
  const instances = ((pasted.instances ?? []) as ListInstance[]).filter((one) => !current.instances.some((each) => each.id === one.id));
  if (!definitions.length && !instances.length) return null;
  return { ...current, definitions: [...current.definitions, ...definitions], instances: [...current.instances, ...instances] };
}
