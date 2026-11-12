// the editable text and pictures of the pages on screen, fetched from the
// engine as pages come into view and kept for the current revision.
//
// after an edit the shell answers the changed page's objects with fresh
// ids, so they are kept straight away (`accept`) instead of being fetched
// again once the new revision arrives.

import { SvelteMap } from "svelte/reactivity";
import type { PageObjects } from "./editApi";

/** pages kept at once; a screen shows a few, this allows for scrolling back. */
const KEPT_PAGES = 30;

export class PageObjectsStore {
  #pages = new SvelteMap<number, PageObjects>();
  #loading = new Set<number>();
  #revision = -1;
  /** objects an edit answered, for the revision that edit made */
  #answered: { revision: number; objects: PageObjects } | null = null;
  #fetch: (page: number) => Promise<PageObjects>;

  constructor(fetch: (page: number) => Promise<PageObjects>) {
    this.#fetch = fetch;
  }

  get(page: number): PageObjects | undefined {
    return this.#pages.get(page);
  }

  get revision(): number {
    return this.#revision;
  }

  /** an edit answered the page as it is in `revision`. */
  accept(revision: number, objects: PageObjects) {
    if (revision === this.#revision) {
      this.#pages.set(objects.page, objects);
    } else {
      this.#answered = { revision, objects };
    }
  }

  /** the document changed: everything is read again, except what the edit already answered. */
  setRevision(revision: number) {
    if (revision === this.#revision) return;
    this.#revision = revision;
    this.#pages.clear();
    this.#loading.clear();
    const answered = this.#answered;
    this.#answered = null;
    if (answered && answered.revision === revision) this.#pages.set(answered.objects.page, answered.objects);
  }

  ensure(pages: number[]) {
    const revision = this.#revision;
    for (const page of pages) {
      if (this.#pages.has(page) || this.#loading.has(page)) continue;
      this.#loading.add(page);
      this.#fetch(page)
        .then((objects) => {
          if (revision !== this.#revision) return;
          this.#loading.delete(page);
          while (this.#pages.size >= KEPT_PAGES) {
            const oldest = this.#pages.keys().next().value;
            if (oldest === undefined) break;
            this.#pages.delete(oldest);
          }
          this.#pages.set(page, objects);
        })
        .catch(() => {
          // a page that can't be read just has nothing to edit
          if (revision === this.#revision) {
            this.#loading.delete(page);
            this.#pages.set(page, { page, width: 0, height: 0, textBlocks: [], images: [] });
          }
        });
    }
  }

  /** reads a page now, for the revision shown (after a stale id, say). */
  async refresh(page: number): Promise<PageObjects> {
    const revision = this.#revision;
    const objects = await this.#fetch(page);
    if (revision === this.#revision) this.#pages.set(page, objects);
    return objects;
  }
}
