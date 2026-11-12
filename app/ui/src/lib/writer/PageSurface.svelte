<script lang="ts">
  // the document as pages on the grey desk, like the pdf viewer: the page
  // size and margins come from the first section, the header sits in the
  // top margin, page boundaries are placed where measured content no
  // longer fits (pagination.ts, measured by pageMeasure.ts), each footnote
  // sits at the bottom of its page, and free objects are placed and
  // wrapped by the object layer (surfaceLayout.ts, objectLayer.ts).
  import "prosemirror-view/style/prosemirror.css";
  import "prosemirror-tables/style/tables.css";
  import "prosemirror-gapcursor/style/gapcursor.css";
  import { onMount, tick } from "svelte";
  import MiniEditor from "./MiniEditor.svelte";
  import type { Block, DocumentMeta, Section } from "./model";
  import { pageGaps, partFor, DESK_GAP } from "./pageGaps";
  import { layoutPages, type LayoutProviderName, type WordLayout } from "./layoutSource";
  import { pagesKey } from "./plugins";
  import { twipsToPx } from "./units";
  import type { WriterSession } from "./writerSession.svelte";
  import { addHeaderFooter } from "./editing";
  import { documentCss } from "./css";
  import type { EditorView } from "prosemirror-view";
  import { assignNotes, blockBoxes, breakPositions, sameGaps, type MeasureContext } from "./pageMeasure";
  import { blockPagesFrom, computeLayer, measureBlocks, measuredBoxHeights, measurePageEdges, samePlacements } from "./surfaceLayout";
  import "./pageContent.css";
  import type { SurfaceLayout } from "./objectGeometry";
  import { objectLayerKey } from "./objectLayer";
  import { sameExclusions } from "./wrap";
  import { wordLineBreaks } from "./wordLines";

  interface Props {
    session: WriterSession;
    zoom: number;
    nightMode: boolean;
    /** Focus Mode: the page alone (the screen fades its controls) */
    focusMode?: boolean;
    /** pages as last measured, for the status bar */
    pageCount?: number;
    /** the page with the cursor, counted from 1 */
    currentPage?: number;
    /** where page breaks come from (layoutSource.ts) */
    layout: { provider: LayoutProviderName; word: WordLayout | null };
  }

  let { session, zoom, nightMode, focusMode = false, pageCount = $bindable(1), currentPage = $bindable(1), layout }: Props = $props();

  let body = $state<HTMLDivElement>();
  let lastPageUsed = $state(0);
  /** each page's top edge in the editor root's coordinates (surfaceLayout.ts) */
  let pageEdges = $state<number[]>([0]);
  /** the footnotes on each page (0-based), in order */
  let notesOnPage = $state<Map<number, string[]>>(new Map());
  /** each footnote's drawn height, for pagination */
  let noteHeights: Record<string, number> = {};

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

  // where the header and footer sit, in px from the page's top and bottom edges.
  // a real header or footer goes at the document's own distance, like word.
  // the add header / add footer buttons have no text of their own, so they are
  // kept inside the margin instead: a document with a small top margin (common
  // in .odt files) would otherwise put the button right over the first line
  const ADD_PART_PX = 24; // the button's min-height, see .add-part below
  const ADD_PART_GAP_PX = 4; // breathing room between the button and the text
  const headerTop = $derived(header ? twipsToPx(margins.header ?? 720) : placeholderOffset(margins.header ?? 720, margins.top));
  const footerBottom = $derived(footer ? twipsToPx(margins.footer ?? 720) : placeholderOffset(margins.footer ?? 720, margins.bottom));

  // the usual distance from the page edge, pulled closer to the edge when the
  // margin is too small to hold the button there, and never off the page
  function placeholderOffset(distanceTwips: number, marginTwips: number): number {
    const roomInMargin = twipsToPx(marginTwips) - ADD_PART_PX - ADD_PART_GAP_PX;
    return Math.max(0, Math.min(twipsToPx(distanceTwips), roomInMargin));
  }
  const footnotes = $derived((meta.notes ?? []).filter((note) => !note.special && note.kind === "footnote"));
  const endnotes = $derived((meta.notes ?? []).filter((note) => !note.special && note.kind === "endnote"));
  // the space left at the bottom of the last page, so it looks like a whole page
  // (its footnotes are drawn in that space, like every page's)
  const lastPageSpace = $derived(Math.max(0, textHeight - lastPageUsed));
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

  // --- measuring pages (pagination.ts), then the objects on them (objectLayer.ts)

  let measureTimer: ReturnType<typeof setTimeout> | null = null;
  /** passes since the document last changed: wrapping text around objects can move them, so a few rounds settle it */
  let settlePasses = 0;

  function scheduleMeasure() {
    if (measureTimer) clearTimeout(measureTimer);
    measureTimer = setTimeout(measure, 120);
  }

  /** what the measuring helpers (pageMeasure.ts) need to know */
  function measureContext(view: EditorView): MeasureContext {
    // footnotes drawn at the bottom of their page tell their height
    for (const note of body?.parentElement?.querySelectorAll<HTMLElement>(".page-notes [data-note-id]") ?? []) {
      noteHeights[note.dataset.noteId ?? ""] = note.getBoundingClientRect().height / zoom;
    }
    return { view, zoom, meta, footnoteIds: new Set(footnotes.map((note) => note.id)), noteHeights };
  }

  function measure() {
    const view = session.view;
    const context = view ? measureContext(view) : null;
    const boxes = context ? blockBoxes(context) : null;
    if (!view || !boxes) return;
    const pages = layoutPages(layout.provider, boxes, textHeight, layout.word);
    // how much of each page is used, for the rest of it to be drawn empty
    const used =
      pages.used ??
      pages.starts.map((start, index) => {
        const end = pages.starts[index + 1] ?? boxes.length;
        return boxes.slice(start, end).reduce((sum, box) => sum + box.height, 0);
      });
    const { positions, insets } = breakPositions(view, pages, zoom);
    const gaps = pageGaps(meta, pages, used, textHeight, positions, insets);
    const current = pagesKey.getState(view.state) ?? [];
    if (!sameGaps(current, gaps)) session.setPageGaps(gaps);
    lastPageUsed = used[used.length - 1] ?? 0;
    notesOnPage = assignNotes(context!, pages);
    // with Word doing the layout, its own page count is the answer
    pageCount = layout.provider !== "estimate" && layout.word ? layout.word.pageCount : pages.starts.length;
    const cursorBlock = view.state.selection.$from.index(0);
    currentPage = pages.starts.filter((start) => start <= cursorBlock).length;
    requestAnimationFrame(placeObjects);
  }

  /** where free objects go and the floats that make text wrap around them; repeats until nothing moves. */
  function placeObjects() {
    const view = session.view;
    if (!view) return;
    const blocks = measureBlocks(view, zoom);
    if (!blocks) return;
    const gaps = pagesKey.getState(view.state) ?? [];
    const edges = measurePageEdges(view, zoom, gaps, twipsToPx(margins.top));
    const surface: SurfaceLayout = {
      pageWidth: page.width,
      pageHeight: page.height,
      margins,
      pageEdges: edges,
      blockPages: blockPagesFrom(gaps, view.state.doc.childCount),
      blockTops: blocks.tops,
    };
    session.objects.layout = surface;
    pageEdges = edges;
    const layer = computeLayer(view.state.doc, surface, twipsToPx(page.width - margins.left - margins.right), measuredBoxHeights(view, view.state.doc, zoom));
    const current = objectLayerKey.getState(view.state);
    const lines = layout.provider !== "estimate" && layout.word?.lines && layout.word === laidOut ? wordLineBreaks(view.state.doc, layout.word.lines) : [];
    const changed = !current || !samePlacements(current.placements, layer.placements) || !sameExclusions(current.exclusions, layer.exclusions) || current.lines.length !== lines.length;
    if (!changed) return;
    view.dispatch(view.state.tr.setMeta(objectLayerKey, { ...layer, lines }).setMeta("addToHistory", false));
    // the wrapped text can push later blocks down: measure the pages again, a few times at most
    if (settlePasses < 3) {
      settlePasses += 1;
      scheduleMeasure();
    }
  }

  // measure after every change of the document, the zoom or the page.
  // placing the page gaps makes a new editor state with the same document,
  // which must not start another measurement
  let measured: { doc: unknown; zoom: number; page: unknown; word: unknown } = { doc: null, zoom: 0, page: null, word: null };
  /** word's answer that has been applied to the lines (only the document it was made for) */
  let laidOut: unknown = null;
  $effect(() => {
    const now = { doc: session.editor.doc, zoom, page, word: layout.word };
    if (now.doc === measured.doc && now.zoom === measured.zoom && now.page === measured.page && now.word === measured.word) return;
    // word's lines are applied when its answer arrives; edits after it keep them where untouched (objectLayer.ts)
    if (now.word !== measured.word) laidOut = now.word;
    measured = now;
    settlePasses = 0;
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
      class:w-focus-mode={focusMode}
      style:width={px(page.width)}
      style:padding="{px(margins.top)} {px(margins.right)} 0 {px(margins.left)}"
      style:--w-margin-left={px(margins.left)}
      style:--w-margin-right={px(margins.right)}
    >
      <div class="part header-part" style:top="{headerTop}px" style:left={px(margins.left)} style:right={px(margins.right)}>
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

      <!-- footnotes at the bottom of the page their reference is on, as Word puts them -->
      {#each [...notesOnPage] as [pageIndex, ids] (pageIndex)}
        <section
          class="notes page-notes"
          aria-label={`Footnotes on page ${pageIndex + 1}`}
          style:top="{(pageEdges[pageIndex] ?? 0) + (page.height - margins.bottom) / 15 + margins.top / 15}px"
          style:left={px(margins.left)}
          style:width={px(page.width - margins.left - margins.right)}
        >
          {#each orderedFootnotes.filter((note) => ids.includes(note.id)) as note (note.id)}
            <div class="note" data-note-id={note.id}>
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
        </section>
      {/each}

      {#if orderedEndnotes.length}
        <section class="notes" aria-label="Endnotes">
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
        <div class="part footer-part" style:bottom="{footerBottom}px" style:left={px(margins.left)} style:right={px(margins.right)}>
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

  /* 24px tall at least (WCAG's target size) and #5f5f66 on the white page,
     6.3:1, where #8a8a8f gave 3.4:1 (docs/ACCESSIBILITY.md) */
  .add-part {
    min-height: 1.5rem;
    padding: 0.25rem 0.5rem;
    border: 0;
    border-radius: 0.375rem;
    background: transparent;
    color: #5f5f66;
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

  /* a page's footnotes sit on its bottom margin line, growing upwards; the
     short rule is word's separator */
  .page-notes {
    position: absolute;
    z-index: 1;
    box-sizing: border-box;
    margin: 0;
    border-top: 0;
    transform: translateY(-100%);
  }

  .page-notes::before {
    content: "";
    display: block;
    width: 2in;
    margin-bottom: 0.4em;
    border-top: 0.5px solid #000000;
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

</style>
