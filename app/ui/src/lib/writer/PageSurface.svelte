<script lang="ts">
  // the document as pages on the grey desk, like the pdf viewer: the page
  // size and margins come from the first section, the header sits in the
  // top margin, page boundaries are placed where measured content no
  // longer fits (an estimate: see pagination.ts), and footnotes are shown
  // together under the text with the last page's footer below them.
  import "prosemirror-view/style/prosemirror.css";
  import "prosemirror-tables/style/tables.css";
  import "prosemirror-gapcursor/style/gapcursor.css";
  import { onMount, tick } from "svelte";
  import MiniEditor from "./MiniEditor.svelte";
  import type { Block, DocumentMeta, Section } from "./model";
  import { pageGaps, partFor, DESK_GAP } from "./pageGaps";
  import type { BlockBox } from "./pagination";
  import { layoutPages, type LayoutProviderName, type WordLayout } from "./layoutSource";
  import type { PageGap } from "./plugins";
  import { pagesKey } from "./plugins";
  import { twipsToPx } from "./units";
  import type { WriterSession } from "./writerSession.svelte";
  import { addHeaderFooter } from "./editing";
  import { documentCss } from "./css";

  interface Props {
    session: WriterSession;
    zoom: number;
    nightMode: boolean;
    /** pages as last measured, for the status bar */
    pageCount?: number;
    /** the page with the cursor, counted from 1 */
    currentPage?: number;
    /** where page breaks come from (layoutSource.ts) */
    layout: { provider: LayoutProviderName; word: WordLayout | null };
  }

  let { session, zoom, nightMode, pageCount = $bindable(1), currentPage = $bindable(1), layout }: Props = $props();

  let body = $state<HTMLDivElement>();
  let lastPageUsed = $state(0);
  let notesHeight = $state(0);

  const meta = $derived(session.editor.doc.attrs.meta as DocumentMeta);
  const section = $derived<Section>(meta.sections?.[0] ?? { page: { width: 12240, height: 15840, margins: { top: 1440, right: 1440, bottom: 1440, left: 1440, header: 720, footer: 720 } } });
  const page = $derived(section.page);
  const margins = $derived(page.margins);
  const textHeight = $derived(twipsToPx(page.height - margins.top - margins.bottom));
  const evenAndOdd = $derived(!!meta.settings?.evenAndOddHeaders);
  const headerId = $derived(partFor(section, section.headers, 1, evenAndOdd));
  const footerId = $derived(partFor(section, section.footers, Math.max(1, pageCount), evenAndOdd));
  const header = $derived((meta.headersFooters ?? []).find((part) => part.id === headerId));
  const footer = $derived((meta.headersFooters ?? []).find((part) => part.id === footerId));
  const footnotes = $derived((meta.notes ?? []).filter((note) => !note.special && note.kind === "footnote"));
  const endnotes = $derived((meta.notes ?? []).filter((note) => !note.special && note.kind === "endnote"));
  // the space left at the bottom of the last page, so it looks like a whole page
  const lastPageSpace = $derived(Math.max(0, textHeight - lastPageUsed - notesHeight));
  const typing = () => session.settings.typing;
  const language = $derived(session.language());

  // the order the notes are numbered in: as their references appear in the text
  const noteOrder = $derived.by(() => {
    const order: string[] = [];
    session.editor.doc.descendants((node) => {
      if (node.type.name === "note_ref") order.push(String((node.attrs.data as { noteId?: string }).noteId));
      return true;
    });
    return order;
  });
  const orderedFootnotes = $derived([...footnotes].sort((a, b) => noteOrder.indexOf(a.id) - noteOrder.indexOf(b.id)));
  const orderedEndnotes = $derived([...endnotes].sort((a, b) => noteOrder.indexOf(a.id) - noteOrder.indexOf(b.id)));

  onMount(() => {
    if (body) return session.mount(body);
  });

  // the document's named styles as one style sheet (css.ts), kept up to date
  const styleSheet = $derived(documentCss(meta));
  onMount(() => {
    const element = document.createElement("style");
    element.dataset.writerStyles = "";
    document.head.append(element);
    const stop = $effect.root(() => {
      $effect(() => {
        element.textContent = styleSheet;
      });
    });
    return () => {
      stop();
      element.remove();
    };
  });

  // --- measuring pages

  let measureTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleMeasure() {
    if (measureTimer) clearTimeout(measureTimer);
    measureTimer = setTimeout(measure, 120);
  }

  /** how tall each top-level block is, with the page gaps taken out. */
  function blockBoxes(): BlockBox[] | null {
    const view = session.view;
    if (!view) return null;
    const doc = view.state.doc;
    const gaps = pagesKey.getState(view.state) ?? [];
    const gapBefore = new Map(gaps.map((gap) => [gap.blockIndex, gap.height]));
    const tops: number[] = [];
    const bottoms: number[] = [];
    let pos = 0;
    for (let index = 0; index < doc.childCount; index += 1) {
      const element = view.nodeDOM(pos) as HTMLElement | null;
      if (!element?.getBoundingClientRect) return null;
      const rect = element.getBoundingClientRect();
      tops.push(rect.top / zoom);
      const style = getComputedStyle(element);
      bottoms.push(rect.bottom / zoom + (Number.parseFloat(style.marginBottom) || 0));
      pos += doc.child(index).nodeSize;
    }
    return tops.map((top, index) => {
      const node = doc.child(index);
      const next = index + 1 < tops.length ? tops[index + 1] - (gapBefore.get(index + 1) ?? 0) : bottoms[index];
      const data = node.attrs.data as { props?: { pageBreakBefore?: boolean } };
      return { height: Math.max(0, next - top), isPageBreak: node.type.name === "page_break", breakBefore: !!data?.props?.pageBreakBefore };
    });
  }

  function measure() {
    const view = session.view;
    const boxes = blockBoxes();
    if (!view || !boxes) return;
    const pages = layoutPages(layout.provider, boxes, textHeight, layout.word);
    // how much of each page is used, for the rest of it to be drawn empty
    const used = pages.starts.map((start, index) => {
      const end = pages.starts[index + 1] ?? boxes.length;
      return boxes.slice(start, end).reduce((sum, box) => sum + box.height, 0);
    });
    const gaps = pageGaps(meta, pages, used, textHeight);
    const current = pagesKey.getState(view.state) ?? [];
    if (!sameGaps(current, gaps)) session.setPageGaps(gaps);
    lastPageUsed = used[used.length - 1] ?? 0;
    // with Word doing the layout, its own page count is the answer
    pageCount = layout.provider === "word" && layout.word ? layout.word.pageCount : pages.starts.length;
    const cursorBlock = view.state.selection.$from.index(0);
    currentPage = pages.starts.filter((start) => start <= cursorBlock).length;
  }

  function sameGaps(a: PageGap[], b: PageGap[]): boolean {
    return a.length === b.length && a.every((gap, index) => gap.blockIndex === b[index].blockIndex && Math.abs(gap.height - b[index].height) < 1);
  }

  // measure after every change of the document, the zoom or the page.
  // placing the page gaps makes a new editor state with the same document,
  // which must not start another measurement
  let measured: { doc: unknown; zoom: number; page: unknown; word: unknown } = { doc: null, zoom: 0, page: null, word: null };
  $effect(() => {
    const now = { doc: session.editor.doc, zoom, page, word: layout.word };
    if (now.doc === measured.doc && now.zoom === measured.zoom && now.page === measured.page && now.word === measured.word) return;
    measured = now;
    tick().then(scheduleMeasure);
  });

  $effect(() => {
    void session.editor.selection;
    const view = session.view;
    if (!view) return;
    const gaps = pagesKey.getState(view.state) ?? [];
    const cursorBlock = view.state.selection.$from.index(0);
    currentPage = 1 + gaps.filter((gap) => gap.blockIndex <= cursorBlock).length;
  });

  onMount(() => {
    document.fonts?.ready.then(scheduleMeasure);
    return () => {
      if (measureTimer) clearTimeout(measureTimer);
    };
  });

  // --- headers and footers

  function addPart(kind: "header" | "footer") {
    const view = session.view;
    if (!view) return;
    const { tr } = addHeaderFooter(view.state, kind);
    session.dispatch(tr);
  }

  function noteNumber(id: string, kind: "footnote" | "endnote"): string {
    const list = kind === "footnote" ? orderedFootnotes : orderedEndnotes;
    const index = list.findIndex((note) => note.id === id) + 1;
    return kind === "endnote" ? roman(index) : String(index);
  }

  function roman(value: number): string {
    const table: [number, string][] = [[10, "x"], [9, "ix"], [5, "v"], [4, "iv"], [1, "i"]];
    let rest = value;
    let out = "";
    for (const [amount, text] of table) while (rest >= amount) ((out += text), (rest -= amount));
    return out;
  }

  const px = (twips: number) => `${twipsToPx(twips)}px`;
</script>

<div class="desk-content" style:padding-bottom="{DESK_GAP * zoom}px">
  <div class="zoom" style:zoom>
    <div
      class="paper"
      class:night={nightMode}
      style:width={px(page.width)}
      style:padding="{px(margins.top)} {px(margins.right)} 0 {px(margins.left)}"
      style:--w-margin-left={px(margins.left)}
      style:--w-margin-right={px(margins.right)}
    >
      <div class="part header-part" style:top={px(margins.header ?? 720)} style:left={px(margins.left)} style:right={px(margins.right)}>
        {#if header}
          <MiniEditor
            blocks={header.blocks}
            label="Header"
            placeholder="Header"
            {typing}
            spellcheck={session.settings.spellcheck}
            {language}
            media={session.media}
            onChange={(blocks: Block[]) => session.setHeaderFooterBlocks(header.id, blocks)}
            onAutocorrect={() => {}}
          />
        {:else}
          <button class="add-part" title="Add text at the top of every page" onclick={() => addPart("header")}>Add Header</button>
        {/if}
      </div>

      <div class="body" bind:this={body}></div>

      {#if orderedFootnotes.length || orderedEndnotes.length}
        <section class="notes" aria-label="Footnotes" bind:clientHeight={notesHeight}>
          {#each orderedFootnotes as note (note.id)}
            <div class="note">
              <span class="note-number" aria-hidden="true">{noteNumber(note.id, "footnote")}</span>
              <MiniEditor
                blocks={note.blocks}
                label={`Footnote ${noteNumber(note.id, "footnote")}`}
                placeholder="Type the footnote"
                {typing}
                spellcheck={session.settings.spellcheck}
                {language}
                media={session.media}
                onChange={(blocks: Block[]) => session.setNoteBlocks(note.id, blocks)}
                onAutocorrect={() => {}}
              />
            </div>
          {/each}
          {#if orderedEndnotes.length}
            <h3 class="notes-title">Endnotes</h3>
            {#each orderedEndnotes as note (note.id)}
              <div class="note">
                <span class="note-number" aria-hidden="true">{noteNumber(note.id, "endnote")}</span>
                <MiniEditor
                  blocks={note.blocks}
                  label={`Endnote ${noteNumber(note.id, "endnote")}`}
                  {typing}
                  spellcheck={session.settings.spellcheck}
                  {language}
                  media={session.media}
                  onChange={(blocks: Block[]) => session.setNoteBlocks(note.id, blocks)}
                  onAutocorrect={() => {}}
                />
              </div>
            {/each}
          {/if}
        </section>
      {/if}

      <div class="last-page-rest" style:height="{lastPageSpace}px"></div>
      <div class="bottom-margin" style:height={px(margins.bottom)}>
        <div class="part footer-part" style:bottom={px(margins.footer ?? 720)} style:left={px(margins.left)} style:right={px(margins.right)}>
          {#if footer}
            <MiniEditor
              blocks={footer.blocks}
              label="Footer"
              placeholder="Footer"
              {typing}
              spellcheck={session.settings.spellcheck}
              {language}
              media={session.media}
              onChange={(blocks: Block[]) => session.setHeaderFooterBlocks(footer.id, blocks)}
              onAutocorrect={() => {}}
            />
          {:else}
            <button class="add-part" title="Add text at the bottom of every page" onclick={() => addPart("footer")}>Add Footer</button>
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .desk-content {
    display: flex;
    justify-content: center;
    min-width: max-content;
    padding-top: 0.5rem;
  }

  /* the page is always white, like paper (Pages does the same in dark mode) */
  .paper {
    position: relative;
    box-sizing: border-box;
    background: #ffffff;
    color: #000000;
    box-shadow: var(--shadow-page);
    /* the document's own default font comes from its styles */
    font: 11pt/1.15 "Calibri", "Carlito", "Segoe UI", sans-serif;
    letter-spacing: normal;
    -webkit-font-smoothing: auto;
  }

  /* night mode turns the page dark, pictures stay as they are */
  .paper.night {
    filter: invert(0.9) hue-rotate(180deg);
  }

  .paper.night :global(img) {
    filter: invert(1) hue-rotate(180deg);
  }

  .part {
    position: absolute;
    min-height: 1.2em;
  }

  .footer-part {
    top: auto;
  }

  .bottom-margin {
    position: relative;
    margin: 0 calc(-1 * var(--w-margin-right)) 0 calc(-1 * var(--w-margin-left));
  }

  .add-part {
    padding: 0.125rem 0.5rem;
    border: 0;
    border-radius: 0.375rem;
    background: transparent;
    color: #8a8a8f;
    font: 400 0.6875rem/0.875rem var(--font);
    opacity: 0;
    transition: opacity 0.15s var(--ease);
  }

  .paper:hover .add-part,
  .add-part:focus-visible {
    opacity: 1;
  }

  .add-part:hover {
    background: rgb(0 0 0 / 0.06);
  }

  .notes {
    margin-top: 1.5em;
    padding-top: 0.5em;
    border-top: 0.5px solid #b0b0b5;
    font-size: 10pt;
  }

  .notes-title {
    margin: 0.75em 0 0.25em;
    font-size: 10pt;
  }

  .note {
    display: flex;
    gap: 0.4em;
    align-items: baseline;
  }

  .note-number {
    flex-shrink: 0;
    font-size: 0.75em;
    vertical-align: super;
  }

  .note :global(.mini) {
    flex: 1;
  }

  /* --- the document's own content */

  .paper :global(.ProseMirror) {
    outline: none;
    word-wrap: break-word;
    white-space: pre-wrap;
    caret-color: #000000;
  }

  /* no specificity at all, so the document's own styles (css.ts) always win */
  :global(:where(.w-doc) :where(p, h1, h2, h3, h4, h5, h6)) {
    margin: 0;
    font-size: inherit;
    font-weight: inherit;
    min-height: 1em;
    /* free pictures are placed from their paragraph's top left */
    position: relative;
  }

  .paper :global(.w-list-item)::before {
    content: attr(data-label);
    display: inline-block;
    min-width: var(--w-hang);
    text-indent: 0;
  }

  .paper :global(.w-tab) {
    white-space: pre;
    tab-size: 48px;
  }

  .paper :global(.w-note-ref)::after {
    content: attr(data-number);
  }

  .paper :global(.w-field) {
    border-radius: 2px;
    background: rgb(0 0 0 / 0.07);
  }

  .paper :global(.w-comment) {
    background: rgb(255 200 0 / 0.3);
    border-bottom: 1px solid rgb(220 160 0);
  }

  /* a small speech-bubble square where a comment's marker sits */
  .paper :global(.w-comment-ref)::before {
    content: "";
    display: inline-block;
    width: 0.55em;
    height: 0.45em;
    margin: 0 0.1em;
    border: 1px solid #c89000;
    border-radius: 2px;
    background: #ffd34d;
    vertical-align: super;
  }

  .paper :global(ins.w-insertion) {
    color: #1a6fd4;
    text-decoration: underline;
    text-decoration-color: #1a6fd4;
  }

  .paper :global(del.w-deletion) {
    color: #c4314b;
    text-decoration: line-through;
  }

  .paper :global(.w-find) {
    background: rgb(255 214 0 / 0.55);
  }

  .paper :global(.w-find-current) {
    background: rgb(255 150 0 / 0.75);
  }

  .paper :global(.w-link) {
    color: #0b57d0;
    text-decoration: underline;
  }

  .paper :global(.w-hidden) {
    text-decoration: underline dotted;
    opacity: 0.5;
  }

  .paper :global(.w-chip) {
    display: inline-block;
    padding: 0 0.4em;
    border: 1px dashed #9a9aa0;
    border-radius: 0.3em;
    background: #f4f4f6;
    color: #5a5a60;
    font: 400 9pt/1.4 var(--font);
    text-indent: 0;
    user-select: none;
  }

  .paper :global(div.w-chip) {
    display: block;
    margin: 0.4em 0;
    padding: 0.6em;
    text-align: center;
  }

  .paper :global(.ProseMirror-selectednode.w-chip),
  .paper :global(.w-image.w-selected img) {
    outline: 2px solid var(--accent);
  }

  .paper :global(.w-page-break),
  .paper :global(.w-section-break) {
    margin: 0.6em 0;
    border-top: 1px dashed #b0b0b5;
    color: #8a8a8f;
    font: 400 8pt/1 var(--font);
    text-align: center;
    user-select: none;
  }

  .paper :global(.w-page-break span),
  .paper :global(.w-section-break span) {
    position: relative;
    top: -0.55em;
    padding: 0 0.5em;
    background: #ffffff;
  }

  .paper :global(.w-image) {
    display: inline-block;
    text-indent: 0;
  }

  .paper :global(.w-image img) {
    display: block;
    max-width: 100%;
    height: auto;
    object-fit: contain;
  }

  .paper :global(.w-image-floating) {
    display: block;
    margin: 0.4em 0;
  }

  .paper :global(.w-image-note) {
    display: block;
    margin-top: 0.2em;
    color: #6a6a70;
    font: 400 8pt/1.2 var(--font);
  }

  .paper :global(.w-image-missing) {
    min-width: 2em;
    min-height: 2em;
    background: repeating-linear-gradient(45deg, #eee, #eee 6px, #f8f8f8 6px, #f8f8f8 12px);
  }

  .paper :global(table.w-table) {
    border-collapse: collapse;
    margin: 0.3em 0;
  }

  .paper :global(table.w-table td),
  .paper :global(table.w-table th) {
    min-width: 2em;
    padding: 0.15em 0.4em;
    border: 0.5px solid #8a8a8f;
    vertical-align: top;
  }

  .paper :global(table.w-table .selectedCell::after) {
    background: rgb(0 136 255 / 0.15);
  }

  .paper :global(.w-page-gap) {
    position: relative;
    margin: 0 calc(-1 * var(--w-margin-right)) 0 calc(-1 * var(--w-margin-left));
    user-select: none;
    pointer-events: none;
  }

  .paper :global(.w-gap-desk) {
    position: absolute;
    left: -8px;
    right: -8px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--desk);
    color: var(--label-tertiary);
    font: 400 0.625rem/1 var(--font);
  }

  .paper :global(.w-gap-header),
  .paper :global(.w-gap-footer) {
    position: absolute;
    left: var(--w-margin-left);
    right: var(--w-margin-right);
    opacity: 0.55;
  }

  @media print {
    .paper {
      box-shadow: none;
      zoom: 1;
    }
    .paper :global(.w-page-gap),
    .add-part {
      display: none;
    }
    .paper :global(.w-page-break) {
      break-after: page;
      border: 0;
      visibility: hidden;
    }
  }
</style>
