// the commands on the page's objects: text blocks (split, make, return
// to text), pictures in line or free, stacking, aligning and moving. the
// ones that change the document's structure run in the office engine
// (shell-core/src/writer/objects.rs) and come back as one transaction
// (textBlocks.ts applyEngineModel), so each is one undo step.

import type { Node as PmNode } from "prosemirror-model";
import { NodeSelection, type EditorState, type Transaction } from "prosemirror-state";
import * as api from "./writerApi";
import { freeObjects, objectRect, pageAt, pageOf, toPageTwips, type FreeObject, type SurfaceLayout } from "./objectGeometry";
import { applyEngineModel, blockPos, boundaryWithoutBox, cursorInTopParagraph, cursorIntoBlock, editorOffset, engineOffset, imageRef, nearestBoundary, selectInline, selectedBlockRange, textBoxAt } from "./textBlocks";
import { snap, type PageBox, type SnapGuide } from "./snapping";
import type { WriterSession } from "./writerSession.svelte";

type Data = Record<string, unknown>;

/** the object the frame acts on: a picture (selected) or the text block holding the cursor. */
export interface SelectedObject {
  kind: "image" | "textBox";
  pos: number;
  node: PmNode;
  free: boolean;
}

export function selectedObject(state: EditorState): SelectedObject | null {
  const selection = state.selection;
  if (selection instanceof NodeSelection && selection.node.type.name === "image") {
    const placement = (selection.node.attrs.data as Data).placement as { type?: string } | undefined;
    return { kind: "image", pos: selection.from, node: selection.node, free: placement?.type === "floating" };
  }
  const box = textBoxAt(state);
  return box ? { kind: "textBox", pos: box.pos, node: box.node, free: true } : null;
}

export class WriterObjects {
  /** the surface's last measurement (PageSurface sets it) */
  layout: SurfaceLayout | null = null;
  /** extra objects picked with Shift for aligning, by position */
  picked = $state<number[]>([]);
  /** where Return to Text would put the text, while its button is pointed at */
  insertionY = $state<number | null>(null);
  private busy = false;

  constructor(private session: WriterSession) {}

  private state(): EditorState {
    return this.session.view?.state ?? this.session.editor;
  }

  /**
   * where top-level block `index` starts on its page, in twips from the
   * page's top: new text blocks are placed there, so they appear where
   * the text was (the engine puts them at the top margin).
   */
  private blockY(index: number): number | null {
    const layout = this.layout;
    const top = layout?.blockTops[index];
    if (!layout || top === undefined || Number.isNaN(top)) return null;
    return toPageTwips(0, top, layout.blockPages[index] ?? pageAt(top, layout), layout).y;
  }

  /** the engine's new text blocks moved down to `y` (page twips), keeping their page-relative place. */
  private static placeBlocks(answer: api.ObjectAnswer, y: number | null) {
    if (y === null || !answer.blocks) return;
    const [start, end] = answer.blocks;
    for (const block of answer.model.blocks.slice(start, end)) {
      if (block.type === "textBox") block.vertical = { relativeTo: "page", offset: y };
    }
  }

  /** runs an engine command and applies its answer as one undo step; `select` puts the cursor afterwards. */
  private async engine(op: api.ObjectOp, select?: (tr: Transaction, answer: api.ObjectAnswer) => void, adjust?: (answer: api.ObjectAnswer) => void): Promise<api.ObjectAnswer | null> {
    if (this.busy) return null;
    this.busy = true;
    const doc = this.state().doc;
    try {
      const answer = await api.objectCommand(this.session.id, this.session.model(), op);
      if (answer.refused) {
        this.session.say(answer.refused);
        return answer;
      }
      if (this.state().doc !== doc) {
        // the person typed while the engine worked: their typing wins
        this.session.say("The document changed while that ran. Try again.");
        return null;
      }
      adjust?.(answer);
      this.session.run((state) => {
        const tr = applyEngineModel(state, answer.model);
        if (tr && select) select(tr, answer);
        return tr;
      });
      return answer;
    } catch (problem) {
      this.session.fail(problem);
      return null;
    } finally {
      this.busy = false;
    }
  }

  // --- text blocks

  /** Split into Text Blocks: the paragraph at the cursor becomes two movable blocks. */
  async split() {
    const at = cursorInTopParagraph(this.state());
    if (!at) {
      this.session.say("Click inside a paragraph of the main text, where it should split.");
      return;
    }
    const y = this.blockY(at.block);
    const answer = await this.engine({ op: "splitIntoTextBoxes", block: at.block, offset: at.offset }, (tr) => cursorIntoBlock(tr, at.block + 1), (result) => WriterObjects.placeBlocks(result, y));
    if (answer?.changed) this.session.announce("Split into two text blocks. Drag either one by its edge to move it.");
  }

  /** Make Text Block: the selected paragraphs move into one text block. */
  async make() {
    const state = this.state();
    if (textBoxAt(state)) {
      this.session.say("The cursor is already in a text block.");
      return;
    }
    const { start, end } = selectedBlockRange(state);
    const y = this.blockY(start);
    const answer = await this.engine({ op: "textBoxFromBlocks", start, end }, (tr) => cursorIntoBlock(tr, start), (result) => WriterObjects.placeBlocks(result, y));
    if (answer?.changed) this.session.announce("Made a text block. Drag it by its edge to move it.");
  }

  /** where Return to Text would put the content: the block boundary nearest the box's top. */
  returnPlace(): { box: number; boundary: number; y: number } | null {
    const box = textBoxAt(this.state());
    const layout = this.layout;
    if (!box || !layout) return null;
    const object = freeObjects(this.state().doc).find((each) => each.pos === box.pos);
    if (!object) return null;
    const rect = objectRect(object, layout);
    const end = lastBottom(layout);
    const boundary = nearestBoundary(layout.blockTops, end, rect.top);
    const y = layout.blockTops.slice(boundary).find((top) => !Number.isNaN(top)) ?? end;
    return { box: box.index, boundary, y };
  }

  /** Return to Text: the text block's content goes back into the flow at the nearest place. */
  async dissolve() {
    const place = this.returnPlace();
    const box = textBoxAt(this.state());
    if (!box) {
      this.session.say("Click inside a text block first.");
      return;
    }
    const at = place ? boundaryWithoutBox(place.boundary, place.box) : box.index;
    this.insertionY = null;
    const answer = await this.engine({ op: "dissolveTextBox", block: box.index, at }, (tr, result) => cursorIntoBlock(tr, result.blocks?.[0] ?? at));
    if (answer?.changed) this.session.announce("The text is back in the flow of the document.");
  }

  // --- pictures: in line or free

  /** In Line or Free through the engine; Free starts exactly where the picture shows now. */
  async setImageFree(free: boolean) {
    const state = this.state();
    const selected = selectedObject(state);
    if (!selected || selected.kind !== "image") {
      this.session.say("Click a picture first.");
      return;
    }
    if (selected.free === free) return;
    const ref = imageRef(state.doc, selected.pos);
    if (!ref) {
      // pictures in tables and text blocks stay with the editor's own toggle
      this.session.setImageFreeLocally(free);
      return;
    }
    if (free) await this.float(ref, selected);
    else await this.unfloat(ref, selected);
  }

  private async float(ref: { block: number; inline: number }, selected: SelectedObject) {
    const hint = this.hintFor(selected.pos, ref.block);
    const previous = ((selected.node.attrs.data as Data).previousPlacement as { wrap?: string } | undefined)?.wrap;
    const answer = await this.engine({ op: "floatImage", block: ref.block, inline: ref.inline, hint, wrap: previous ?? "square" }, (tr) => selectInline(tr, ref.block, ref.inline));
    if (answer?.changed) this.session.announce("The picture is free: drag it anywhere on the page. Text flows around it.");
  }

  private async unfloat(ref: { block: number; inline: number }, selected: SelectedObject) {
    const to = this.textPlaceUnder(selected.pos, ref);
    const answer = await this.engine({ op: "inlineImage", block: ref.block, inline: ref.inline, to }, (tr, result) => {
      const [block, inline] = result.image ?? [ref.block, ref.inline];
      selectInline(tr, block, inline);
    });
    if (answer?.changed) this.session.announce("The picture is in line with the text.");
  }

  /** the picture's top-left on its paragraph's page, in twips: float_image's hint. */
  private hintFor(pos: number, block: number): [number, number] | null {
    const view = this.session.view;
    const layout = this.layout;
    const element = view?.nodeDOM(pos) as HTMLElement | null;
    const image = element?.querySelector(".w-crop, img") as HTMLElement | null;
    if (!view || !layout || !image) return null;
    const { left, top } = rootPoint(view.dom, image.getBoundingClientRect(), this.session.zoom);
    const page = layout.blockPages[block] ?? pageAt(top, layout);
    const twips = toPageTwips(left, top, page, layout);
    return [twips.x, twips.y];
  }

  /** the text place nearest the free picture's middle, as (block, character offset) once the picture is out. */
  private textPlaceUnder(pos: number, ref: { block: number; inline: number }): [number, number] | null {
    const view = this.session.view;
    const element = view?.nodeDOM(pos) as HTMLElement | null;
    if (!view || !element) return null;
    const box = element.getBoundingClientRect();
    const target = view.posAtCoords({ left: box.left + box.width / 2, top: box.top + box.height / 2 });
    if (!target) return null;
    const spot = view.state.doc.resolve(target.pos);
    if (spot.depth !== 1 || !spot.parent.isTextblock) return null;
    const block = spot.index(0);
    let offset = engineOffset(spot.parent, spot.parentOffset);
    // the engine takes the picture out first: a place after it in the same paragraph moves back one
    if (block === ref.block) {
      const imageOffset = engineOffset(spot.parent, view.state.doc.resolve(pos).parentOffset);
      if (offset > imageOffset) offset -= 1;
    }
    return [block, offset];
  }

  // --- stacking, aligning, distributing

  private refFor(object: FreeObject): api.ObjectRef | null {
    if (object.kind === "textBox") return { type: "textBox", block: object.block };
    const ref = imageRef(this.state().doc, object.pos);
    return ref;
  }

  private selectionRefs(): api.ObjectRef[] {
    const state = this.state();
    const selected = selectedObject(state);
    const positions = [...new Set([...(selected ? [selected.pos] : []), ...this.picked])];
    const objects = freeObjects(state.doc).filter((object) => positions.includes(object.pos));
    return objects.map((object) => this.refFor(object)).filter((ref): ref is api.ObjectRef => !!ref);
  }

  async stack(toFront: boolean) {
    const [object] = this.selectionRefs();
    if (!object) {
      this.session.say("Select a free picture or a text block first.");
      return;
    }
    const keep = this.state().selection;
    const answer = await this.engine(toFront ? { op: "bringToFront", object } : { op: "sendToBack", object }, (tr) => {
      if (keep.from <= tr.doc.content.size) tr.setSelection(keep.map(tr.doc, tr.mapping));
    });
    if (answer?.changed) this.session.announce(toFront ? "Brought to front." : "Sent to back.");
  }

  async align(edge: "left" | "center" | "right" | "top" | "middle" | "bottom") {
    const objects = this.selectionRefs();
    if (objects.length < 2) {
      this.session.say("Select two or more objects to line up: click one, then Shift-click the others.");
      return;
    }
    const answer = await this.engine({ op: "align", objects, edge });
    if (answer?.changed) this.session.announce("Lined up.");
  }

  async distribute(horizontal: boolean) {
    const objects = this.selectionRefs();
    if (objects.length < 3) {
      this.session.say("Select three or more objects to space evenly: click one, then Shift-click the others.");
      return;
    }
    const answer = await this.engine({ op: "distribute", objects, horizontal });
    if (answer?.changed) this.session.announce("Spaced evenly.");
  }

  /** Shift-click on an object adds it to (or takes it out of) what Align acts on. */
  togglePicked(pos: number) {
    this.picked = this.picked.includes(pos) ? this.picked.filter((each) => each !== pos) : [...this.picked, pos];
  }

  // --- moving: the snap guides while dragging, and the move itself

  /** snaps a proposed box (root coordinates) on `page` to the page and the other objects; guides in page twips. */
  snapBox(moving: number, proposed: { left: number; top: number; width: number; height: number }, page: number, threshold: number): { left: number; top: number; guides: SnapGuide[] } {
    const layout = this.layout;
    if (!layout) return { left: proposed.left, top: proposed.top, guides: [] };
    const corner = toPageTwips(proposed.left, proposed.top, page, layout);
    const box: PageBox = { x: corner.x, y: corner.y, width: Math.round(proposed.width * 15), height: Math.round(proposed.height * 15) };
    const others = freeObjects(this.state().doc)
      .filter((object) => object.pos !== moving && pageOf(object, layout) === page)
      .map((object) => {
        const rect = objectRect(object, layout);
        const at = toPageTwips(rect.left, rect.top, page, layout);
        return { x: at.x, y: at.y, width: object.width, height: object.height };
      });
    const snapped = snap({ width: layout.pageWidth, height: layout.pageHeight, margins: layout.margins }, others, box, threshold);
    const edge = layout.pageEdges[page] ?? 0;
    return { left: snapped.rect.x / 15 - layout.margins.left / 15, top: edge + snapped.rect.y / 15, guides: snapped.guides };
  }

  /**
   * moves the object at `pos` so its top-left is at root (`left`, `top`):
   * it is then placed relative to the page (as the engine's move_to does).
   * dropped on another page, its anchor moves to that page's first paragraph.
   */
  moveTo(pos: number, left: number, top: number) {
    const layout = this.layout;
    if (!layout) return;
    const page = pageAt(top, layout);
    const { x, y } = toPageTwips(left, top, page, layout);
    const position = { horizontal: { relativeTo: "page", offset: x }, vertical: { relativeTo: "page", offset: y } };
    this.session.run((state) => {
      const node = state.doc.nodeAt(pos);
      if (!node) return null;
      const data = node.attrs.data as Data;
      const tr = state.tr;
      if (node.type.name === "text_box") {
        tr.setNodeMarkup(pos, undefined, { data: { ...data, ...position } });
      } else {
        const placement = data.placement as Data;
        tr.setNodeMarkup(pos, undefined, { data: { ...data, placement: { ...placement, ...position } } }, node.marks);
      }
      const moved = reanchor(tr, pos, node.type.name === "text_box", page, layout);
      if (node.type.name === "image") tr.setSelection(NodeSelection.create(tr.doc, moved));
      return tr;
    }, false);
  }

  /** one undo step for a size, crop or turn of the object at `pos`. */
  setData(pos: number, change: (data: Data) => Data) {
    this.session.run((state) => {
      const node = state.doc.nodeAt(pos);
      if (!node) return null;
      const tr = state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, data: change({ ...(node.attrs.data as Data) }) }, node.marks);
      if (node.type.name === "image") tr.setSelection(NodeSelection.create(tr.doc, pos));
      return tr;
    }, false);
  }
}

/** where the flow ends, for the boundary after the last block. */
function lastBottom(layout: SurfaceLayout): number {
  const tops = layout.blockTops.filter((top) => !Number.isNaN(top));
  return (tops[tops.length - 1] ?? 0) + 20;
}

/** a screen rectangle in the editor root's coordinates (css pixels at 100%). */
export function rootPoint(root: HTMLElement, rect: { left: number; top: number }, zoom: number): { left: number; top: number } {
  const origin = root.getBoundingClientRect();
  return { left: (rect.left - origin.left) / zoom, top: (rect.top - origin.top) / zoom };
}

/**
 * an object moved to another page belongs to that page's first paragraph
 * (word positions it relative to its anchor's page). returns its new position.
 */
function reanchor(tr: Transaction, pos: number, isBox: boolean, page: number, layout: SurfaceLayout): number {
  const block = tr.doc.resolve(pos).index(0);
  const current = isBox ? (layout.blockPages[block + 1] ?? layout.blockPages[block]) : layout.blockPages[block];
  if (current === page) return pos;
  const target = layout.blockPages.findIndex((each, index) => each === page && !Number.isNaN(layout.blockTops[index]) && !!tr.doc.maybeChild(index)?.isTextblock);
  const node = tr.doc.nodeAt(pos);
  if (target < 0 || target === block || !node) return pos;
  const start = blockPos(tr.doc, target);
  tr.delete(pos, pos + node.nodeSize);
  const at = tr.mapping.maps[tr.mapping.maps.length - 1].map(start, -1);
  // a text block goes right before its new anchor paragraph; a picture into it
  const place = isBox ? at : at + 1;
  tr.insert(place, node);
  return place;
}
export { editorOffset };
