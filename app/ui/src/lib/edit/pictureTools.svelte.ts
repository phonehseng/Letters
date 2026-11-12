// the picture half of the Edit tab: the selected picture and every change
// to it (move, resize, turn, restack, replace, delete, save out), and new
// pictures from a file, the clipboard or a drop. the edit session holds
// one of these as `pictures` and lends it the parts it shares (the page
// objects, the change queue, the page tool).

import { pictureSize } from "../annotate/annotateApi";
import type { DocumentState } from "../api";
import type { PageSize } from "../layout";
import { rectAround } from "../pageTools/gestures";
import type { PagePoint, Rect } from "../pageTools/pagePoints";
import { isUpright, matchingImage } from "./blocks";
import { editApi, type ImageEditAnswer, type ImageFit, type PageObjects } from "./editApi";
import type { PageObjectsStore } from "./pageObjectsStore.svelte";

/** a picture placed with a click, or from the keyboard, is two inches wide. */
export const NEW_PICTURE_WIDTH = 144;

/** what the edit session lends the picture tools. */
export interface PictureHost {
  documentId: () => number;
  pageSize: (page: number) => PageSize | undefined;
  viewCentre: () => PagePoint;
  /** runs a change through the window's queue; false when it failed (already shown) */
  change: (action: () => Promise<DocumentState>) => Promise<boolean>;
  announce: (text: string) => void;
  showError: (message: string) => void;
  focusDocument: () => void;
  objects: PageObjectsStore;
  /** keeps an edit's fresh objects for the revision it made */
  keep: (revision: number, objects: PageObjects, stale: boolean) => void;
  /** the paragraph being typed in is finished first, and its notice put away */
  finishTyping: () => void;
  /** makes Edit Text and Images (or Add Image) the page tool */
  useTool: (tool: "edit" | "addImage") => void;
}

/** an image change, given the picture's current id. */
type ImageCall = (id: number, page: number, imageId: string) => Promise<ImageEditAnswer>;

export class PictureTools {
  #host: PictureHost;

  selected = $state<{ page: number; id: string } | null>(null);
  /** where the selected picture's frame is drawn while it is dragged or nudged */
  preview = $state<Rect | null>(null);
  /** the picture chosen for Add Image, with its width over height */
  chosen = $state<{ path: string; aspect: number } | null>(null);
  /** how a replacement picture fills the old one's place */
  replaceFit = $state<ImageFit>("fit");

  readonly image = $derived.by(() => {
    const selected = this.selected;
    if (!selected) return null;
    const images = this.#host.objects.get(selected.page)?.images ?? [];
    return images.find((image) => image.id === selected.id) ?? null;
  });

  constructor(host: PictureHost) {
    this.#host = host;
  }

  select(page: number, id: string) {
    this.#host.finishTyping();
    this.selected = { page, id };
    this.preview = null;
  }

  deselect() {
    this.selected = null;
    this.preview = null;
  }

  /** one picture change through the queue; a stale id finds the picture again by place once. */
  async #run(page: number, call: (imageId: string) => Promise<ImageEditAnswer>, imageId: string, place: Rect | null): Promise<boolean> {
    let answer: ImageEditAnswer | null = null;
    const worked = await this.#host.change(async () => {
      answer = await call(imageId);
      // the fresh ids are kept, and the picture selected by its new one,
      // before the viewer shows the new revision, so its frame stays put
      this.#host.keep(answer.document.revision, answer.objects, answer.stale);
      if (!answer.stale) this.selected = answer.imageId ? { page, id: answer.imageId } : null;
      return answer.document;
    });
    if (!worked || !answer) return false;
    const result: ImageEditAnswer = answer;
    if (!result.stale) {
      this.preview = null;
      return true;
    }
    const again = place ? matchingImage(result.objects, place) : null;
    if (!again) return false;
    this.selected = { page, id: again.id };
    return this.#run(page, call, again.id, null);
  }

  /** the frame was dragged or nudged to `rect`: a move, or a resize when its size changed. */
  async place(rect: Rect): Promise<boolean> {
    const image = this.image;
    const selected = this.selected;
    if (!image || !selected) return false;
    this.preview = rect;
    const id = this.#host.documentId();
    const sameSize = Math.abs(rect.width - image.rect.width) < 0.01 && Math.abs(rect.height - image.rect.height) < 0.01;
    // a turned picture's box isn't its shape, so it can only be moved
    const moveOnly = sameSize || !isUpright(image);
    const dx = rect.x - image.rect.x;
    const dy = rect.y - image.rect.y;
    const call = moveOnly
      ? (imageId: string) => editApi.moveImage(id, selected.page, imageId, dx, dy)
      : (imageId: string) => editApi.resizeImage(id, selected.page, imageId, rect);
    const worked = await this.#run(selected.page, call, image.id, image.rect);
    if (!worked) {
      this.preview = null;
      return false;
    }
    this.#host.announce(`Picture ${moveOnly ? "moved" : "resized"} on page ${selected.page + 1}.`);
    return true;
  }

  rotate(degrees: number): Promise<boolean> {
    return this.#action((id, page, imageId) => editApi.rotateImage(id, page, imageId, degrees), "Picture turned");
  }

  restack(toFront: boolean): Promise<boolean> {
    const said = toFront ? "Picture brought to the front" : "Picture sent to the back";
    return this.#action((id, page, imageId) => editApi.restackImage(id, page, imageId, toFront), said);
  }

  replace(path: string): Promise<boolean> {
    const fit = this.replaceFit;
    return this.#action((id, page, imageId) => editApi.replaceImage(id, page, imageId, path, fit), "Picture replaced");
  }

  async delete(): Promise<boolean> {
    const worked = await this.#action((id, page, imageId) => editApi.deleteImage(id, page, imageId), "Picture deleted");
    if (worked) {
      this.deselect();
      this.#host.focusDocument();
    }
    return worked;
  }

  async #action(call: ImageCall, said: string): Promise<boolean> {
    const image = this.image;
    const selected = this.selected;
    if (!image || !selected) return false;
    const id = this.#host.documentId();
    const worked = await this.#run(selected.page, (imageId) => call(id, selected.page, imageId), image.id, image.rect);
    if (worked) this.#host.announce(`${said} on page ${selected.page + 1}.`);
    return worked;
  }

  /** Save Image As…: the picture as its own file. answers the path written. */
  async save(path: string): Promise<string | null> {
    const image = this.image;
    const selected = this.selected;
    if (!image || !selected) return null;
    try {
      return await editApi.saveImage(this.#host.documentId(), selected.page, image.id, path);
    } catch (problem) {
      this.#host.showError(messageOf(problem));
      return null;
    }
  }

  /** a picture was chosen for Add Image: clicking or dragging on a page places it. */
  async choose(path: string): Promise<boolean> {
    try {
      const size = await pictureSize(path);
      this.chosen = { path, aspect: size.width / Math.max(size.height, 1) };
      this.#host.useTool("addImage");
      return true;
    } catch (problem) {
      this.#host.showError(messageOf(problem));
      return false;
    }
  }

  /** the size a click places the chosen picture at, keeping its proportions. */
  clickSize(): { width: number; height: number } | null {
    if (!this.chosen) return null;
    return { width: NEW_PICTURE_WIDTH, height: NEW_PICTURE_WIDTH / this.chosen.aspect };
  }

  async placeChosen(page: number, rect: Rect): Promise<boolean> {
    const chosen = this.chosen;
    if (!chosen) return false;
    const worked = await this.#add(page, rect, { path: chosen.path });
    // a picture is placed one at a time; then it is selected for moving
    if (worked) this.#host.useTool("edit");
    return worked;
  }

  /** puts the chosen picture in the middle of the view, for the keyboard. */
  async placeChosenInView(): Promise<boolean> {
    const size = this.clickSize();
    if (!size) return false;
    const centre = this.#host.viewCentre();
    const pageSize = this.#host.pageSize(centre.page);
    if (!pageSize) return false;
    return this.placeChosen(centre.page, rectAround(centre, size.width, size.height, pageSize));
  }

  /** a picture dropped on a page from the system, centred where it landed. */
  async drop(path: string, at: PagePoint): Promise<boolean> {
    const pageSize = this.#host.pageSize(at.page);
    if (!pageSize) return false;
    try {
      const pixels = await pictureSize(path);
      const height = NEW_PICTURE_WIDTH / (pixels.width / Math.max(pixels.height, 1));
      return this.#add(at.page, rectAround(at, NEW_PICTURE_WIDTH, height, pageSize), { path });
    } catch (problem) {
      this.#host.showError(messageOf(problem));
      return false;
    }
  }

  /** a picture pasted from the clipboard, in the middle of the view. */
  async paste(bytes: Uint8Array, aspect: number): Promise<boolean> {
    const centre = this.#host.viewCentre();
    const pageSize = this.#host.pageSize(centre.page);
    if (!pageSize) return false;
    const height = NEW_PICTURE_WIDTH / Math.max(aspect, 0.01);
    const rect = rectAround(centre, NEW_PICTURE_WIDTH, height, pageSize);
    return this.#add(centre.page, rect, { bytes: Array.from(bytes) });
  }

  async #add(page: number, rect: Rect, source: { path: string } | { bytes: number[] }): Promise<boolean> {
    const id = this.#host.documentId();
    let answer: ImageEditAnswer | null = null;
    const worked = await this.#host.change(async () => {
      answer = await editApi.addImage(id, page, rect, source);
      this.#host.keep(answer.document.revision, answer.objects, false);
      return answer.document;
    });
    if (!worked || !answer) return false;
    const result: ImageEditAnswer = answer;
    this.#host.useTool("edit");
    if (result.imageId) this.selected = { page, id: result.imageId };
    this.#host.announce(`Picture added to page ${page + 1}.`);
    return true;
  }
}

function messageOf(problem: unknown): string {
  if (typeof problem === "object" && problem !== null && "message" in problem) return String((problem as { message: unknown }).message);
  return String(problem);
}
