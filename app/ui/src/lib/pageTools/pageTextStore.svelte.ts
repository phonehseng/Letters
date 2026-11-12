// the text of the pages on screen, fetched from the engine as pages come
// into view and kept until the document changes.
//
// any page tool that works with text (Select, Highlight, and later
// "select text to redact") reads it from here. a page's text is fetched
// once per revision of the document; only the pages near the screen are
// asked for, and only the most recent few dozen are kept.

import { SvelteMap } from "svelte/reactivity";
import type { PageText } from "./textSelection";

/** pages whose text is kept. a screen shows a few; this allows for scrolling back. */
const KEPT_PAGES = 40;

export class PageTextStore {
  #texts = new SvelteMap<number, PageText>();
  #loading = new Set<number>();
  #revision = -1;
  #fetch: (page: number) => Promise<PageText>;

  constructor(fetch: (page: number) => Promise<PageText>) {
    this.#fetch = fetch;
  }

  /** the text of a page, if it has arrived. reading it is reactive. */
  get(page: number): PageText | undefined {
    return this.#texts.get(page);
  }

  /** forgets everything when the document has changed. */
  setRevision(revision: number) {
    if (revision === this.#revision) return;
    this.#revision = revision;
    this.#texts.clear();
    this.#loading.clear();
  }

  /** asks for the text of these pages, unless it is already here or on its way. */
  ensure(pages: number[]) {
    const revision = this.#revision;
    for (const page of pages) {
      if (this.#texts.has(page) || this.#loading.has(page)) continue;
      this.#loading.add(page);
      this.#fetch(page)
        .then((text) => {
          // an answer about an older version of the document is thrown away
          if (revision !== this.#revision) return;
          this.#loading.delete(page);
          this.#keepRoom();
          this.#texts.set(page, text);
        })
        .catch(() => {
          // a page whose text can't be read just has none to select
          if (revision === this.#revision) this.#texts.set(page, { chars: [], lines: [] });
        });
    }
  }

  /** the oldest pages go once too many are kept. */
  #keepRoom() {
    while (this.#texts.size >= KEPT_PAGES) {
      const oldest = this.#texts.keys().next().value;
      if (oldest === undefined) return;
      this.#texts.delete(oldest);
    }
  }
}
