// the vector shapes half of the Edit tab: the selected shapes (one path,
// or a suggested group such as a logo, which moves as one) and every
// change to them: move, resize, turn, restyle, edit points, restack,
// duplicate, delete. the edit session holds one of these as `shapes`, the
// way it holds the pictures, and lends it the page objects, the change
// queue and the consent for signed files.

import type { DocumentState } from "../api";
import type { Point, Rect } from "../pageTools/pagePoints";
import { summarySentence } from "./changeSentences";
import type { ConsentGate } from "./consent.svelte";
import { contentApi, type ContentAnswer, type EditConsent, type PathStyle } from "./contentApi";
import type { PageObjects } from "./editApi";
import type { PageObjectsStore } from "./pageObjectsStore.svelte";
import { selectionFor, shapesOf, unionRect } from "./shapeGeometry";

/** what the edit session lends the shape tools. */
export interface ShapeHost {
  documentId: () => number;
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  announce: (text: string) => void;
  notify: (text: string) => void;
  focusDocument: () => void;
  objects: PageObjectsStore;
  keep: (revision: number, objects: PageObjects, stale: boolean) => void;
  consent: ConsentGate;
  /** the paragraph being typed in is finished, and other selections let go */
  finishTyping: () => void;
}

/** a shape change, given the shapes' current ids and the consent. */
type ShapeCall = (id: number, page: number, ids: string[], consent: EditConsent) => Promise<ContentAnswer>;

export class ShapeTools {
  #host: ShapeHost;

  selected = $state<{ page: number; ids: string[] } | null>(null);
  /** where the frame is drawn while it is dragged or nudged */
  preview = $state<Rect | null>(null);
  /** a point of a line or rectangle being dragged, and where to */
  pointPreview = $state<Point[] | null>(null);

  readonly shapes = $derived.by(() => {
    const selected = this.selected;
    return selected ? shapesOf(this.#host.objects.get(selected.page), selected.ids) : [];
  });
  readonly rect = $derived(this.shapes.length > 0 ? unionRect(this.shapes.map((shape) => shape.rect)) : null);
  /** one shape whose points can be edited: a rectangle or a line */
  readonly pointShape = $derived(this.shapes.length === 1 && this.shapes[0].pointEditable ? this.shapes[0] : null);

  constructor(host: ShapeHost) {
    this.#host = host;
  }

  /** a click on a shape selects it, or the group it belongs to. */
  select(page: number, pathId: string) {
    this.#host.finishTyping();
    const objects = this.#host.objects.get(page);
    this.selected = { page, ids: objects ? selectionFor(objects, pathId) : [pathId] };
    this.preview = null;
    this.pointPreview = null;
  }

  deselect() {
    this.selected = null;
    this.preview = null;
    this.pointPreview = null;
  }

  /** one change through the queue; a stale id selects the shapes again by place, once. */
  async #run(call: ShapeCall, said: string): Promise<boolean> {
    const selected = this.selected;
    const place = this.rect;
    if (!selected || !place) return false;
    const answer = await this.#send(selected.page, selected.ids, call);
    if (!answer) return false;
    if (answer.stale) {
      const again = (answer.objects.paths ?? []).find((path) => Math.abs(path.rect.x - place.x) < 1 && Math.abs(path.rect.y - place.y) < 1);
      if (!again) return false;
      this.select(selected.page, again.id);
      const retried = await this.#send(selected.page, this.selected!.ids, call);
      if (!retried || retried.stale) return false;
      return this.#done(selected.page, retried, said);
    }
    return this.#done(selected.page, answer, said);
  }

  async #send(page: number, ids: string[], call: ShapeCall): Promise<ContentAnswer | null> {
    let answer: ContentAnswer | null = null;
    const id = this.#host.documentId();
    const worked = await this.#host.change(async () => {
      const answered = await call(id, page, ids, this.#host.consent.consent);
      answer = answered;
      this.#host.keep(answered.document.revision, answered.objects, answered.stale);
      // the shapes are selected by their new ids before the new revision shows
      if (!answered.stale) this.selected = answered.objectIds.length > 0 ? { page, ids: answered.objectIds } : null;
      return answered.document;
    });
    return worked ? answer : null;
  }

  #done(page: number, answer: ContentAnswer, said: string): boolean {
    this.preview = null;
    this.pointPreview = null;
    this.#host.announce(`${said} on page ${page + 1}.`);
    const quiet = summarySentence(answer.summary);
    if (quiet) this.#host.notify(quiet);
    return true;
  }

  /** the frame was dragged or nudged to `rect`: a move, or a resize when its size changed. */
  place(rect: Rect): Promise<boolean> {
    const from = this.rect;
    if (!from) return Promise.resolve(false);
    this.preview = rect;
    const sameSize = Math.abs(rect.width - from.width) < 0.01 && Math.abs(rect.height - from.height) < 0.01;
    const dx = rect.x - from.x;
    const dy = rect.y - from.y;
    if (sameSize) return this.#run((id, page, ids, consent) => contentApi.movePaths(id, page, ids, dx, dy, consent), "Shape moved");
    return this.#run((id, page, ids, consent) => contentApi.resizePaths(id, page, ids, rect, consent), "Shape resized");
  }

  rotate(degrees: number): Promise<boolean> {
    return this.#run((id, page, ids, consent) => contentApi.rotatePaths(id, page, ids, degrees, consent), "Shape turned");
  }

  restyle(style: PathStyle): Promise<boolean> {
    return this.#run((id, page, ids, consent) => contentApi.stylePaths(id, page, ids, style, consent), "Shape restyled");
  }

  setPoints(points: Point[]): Promise<boolean> {
    const shape = this.pointShape;
    if (!shape) return Promise.resolve(false);
    this.pointPreview = points;
    return this.#run((id, page, _ids, consent) => contentApi.pathPoints(id, page, shape.id, points, consent), "Shape reshaped");
  }

  restack(toFront: boolean): Promise<boolean> {
    const said = toFront ? "Shape brought to the front" : "Shape sent to the back";
    return this.#run((id, page, ids, consent) => contentApi.restackPaths(id, page, ids, toFront, consent), said);
  }

  duplicate(): Promise<boolean> {
    return this.#run((id, page, ids, consent) => contentApi.duplicatePaths(id, page, ids, consent), "Shape duplicated");
  }

  async delete(): Promise<boolean> {
    const worked = await this.#run((id, page, ids, consent) => contentApi.deletePaths(id, page, ids, consent), "Shape deleted");
    if (worked) {
      this.deselect();
      this.#host.focusDocument();
    }
    return worked;
  }
}
