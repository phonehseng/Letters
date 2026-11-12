<script lang="ts">
  // the Font Library: every font on this computer, the person's own, the
  // ones that come with Letters and the online library's thousands, in
  // one searchable list. it floats over the window like a sheet; the
  // window waits behind it, Escape or Done closes it.
  //
  // the list is virtual (virtualList.ts): only the rows on screen exist,
  // so two thousand families scroll smoothly. searching happens in the
  // engine (core/fonts search.rs) on every keystroke; only the newest
  // answer is shown.
  import { onMount } from "svelte";
  import { setFileDropTarget } from "../fileDrop";
  import Icon from "../Icon.svelte";
  import { tooltipFor, tools } from "../tools";
  import { addedSentence, chooseFontFiles, chooseFontFolder, fontFilesAmong } from "./fontFiles";
  import { fontsApi, type FamilySummary, type MissingFont, type SearchResults } from "./fontsApi";
  import {
    CATEGORY_CHIPS,
    CATEGORY_LABELS,
    EMPTY_FILTERS,
    PLACE_CHIPS,
    PLACE_LABELS,
    familyMeta,
    latestOnly,
    queryOf,
    resultSentence,
    sizeWords,
    toggle,
    type FilterState,
  } from "./fontQuery";
  import { fontStore } from "./fontStore.svelte";
  import FontDetail from "./FontDetail.svelte";
  import FontSample from "./FontSample.svelte";
  import MissingFonts from "./MissingFonts.svelte";
  import { SIZE_RANGE, clampSize, sampleFor } from "./sampleText";
  import { libraryRowHeight, moveWithKey, rowsPerPage, scrollToShow, visibleRows } from "./virtualList";

  interface Props {
    /** fonts the open document uses, for the Missing Fonts view */
    documentFonts?: string[];
    /** replaces a font in the open document; Replace is offered only with it */
    onReplaceFont?: (from: string, to: string) => void;
    /** a family to show first */
    initialFamily?: string;
    /** open on the Missing Fonts view */
    initialView?: "all" | "missing";
    onClose: () => void;
  }

  let { documentFonts = [], onReplaceFont, initialFamily, initialView = "all", onClose }: Props = $props();

  const store = fontStore();
  let filters = $state<FilterState>({ ...EMPTY_FILTERS });
  let size = $state<number>(SIZE_RANGE.initial);
  let results = $state<SearchResults>({ families: [], understood: [], metricCompatible: [] });
  let resultsRevision = $state(-1);
  let selected = $state(-1);
  let scrollTop = $state(0);
  let viewport = $state(400);
  let view = $state<"all" | "missing">("all");
  let missing = $state<MissingFont[]>([]);
  let announcement = $state("");
  let dropping = $state(false);
  let list = $state<HTMLDivElement>();
  let searchField = $state<HTMLInputElement>();

  const rowHeight = $derived(libraryRowHeight(size));
  const shown = $derived(results.families.map((position) => store.families[position]).filter(Boolean) as FamilySummary[]);
  const rows = $derived(visibleRows(scrollTop, viewport, rowHeight, shown.length));
  const selectedFamily = $derived(shown[selected] ?? null);
  const countSentence = $derived(resultSentence(shown.length, filters, store.scanning));
  const metricNames = $derived(new Set(results.metricCompatible.map((position) => store.families[position]?.name)));

  const runSearch = latestOnly(fontsApi.search);

  async function search() {
    const query = queryOf(filters);
    const answer = await runSearch(query);
    if (!answer.current) return;
    // a search answer refers to positions in one revision of the list
    if (answer.value.revision !== store.revision) await store.loadFamilies();
    const previous = selectedFamily?.name;
    results = answer.value.results;
    resultsRevision = answer.value.revision;
    const kept = previous ? results.families.findIndex((position) => store.families[position]?.name === previous) : -1;
    selected = kept >= 0 ? kept : results.families.length ? 0 : -1;
  }

  // a new search whenever the filters change or the library does
  $effect(() => {
    void JSON.stringify(filters);
    void store.revision;
    void search();
  });

  async function loadMissing() {
    missing = documentFonts.length ? await fontsApi.missing(documentFonts) : [];
  }

  $effect(() => {
    void store.revision;
    void loadMissing();
  });

  function select(index: number, scroll = true) {
    selected = index;
    if (scroll && list) {
      list.scrollTop = scrollToShow(index, list.scrollTop, list.clientHeight, rowHeight);
    }
  }

  function onListKey(event: KeyboardEvent) {
    const next = moveWithKey(event.key, selected, shown.length, rowsPerPage(viewport, rowHeight));
    if (next === null) return;
    event.preventDefault();
    select(next);
  }

  function onSearchKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" && shown.length) {
      event.preventDefault();
      list?.focus();
      select(Math.max(0, selected));
    }
  }

  function showFamily(name: string) {
    // a family named from the detail pane (a look-alike) may be filtered out
    let index = shown.findIndex((family) => family.name === name);
    if (index < 0) {
      filters = { ...EMPTY_FILTERS, text: name, sample: filters.sample };
      pendingName = name;
      return;
    }
    select(index);
  }

  let pendingName = $state<string | null>(null);
  onMount(() => {
    pendingName = initialFamily ?? null;
  });
  $effect(() => {
    if (!pendingName || resultsRevision < 0) return;
    const index = shown.findIndex((family) => family.name.toLowerCase() === pendingName!.toLowerCase());
    if (index >= 0) {
      pendingName = null;
      select(index);
    }
  });

  async function addPaths(paths: string[]) {
    if (!paths.length) return;
    announcement = "Adding fonts…";
    const report = await fontsApi.add(paths);
    announcement = addedSentence(report);
    await store.refresh();
  }

  async function addFiles() {
    await addPaths(await chooseFontFiles());
  }

  async function addFolder() {
    const folder = await chooseFontFolder();
    if (folder) await addPaths([folder]);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" && !event.defaultPrevented) {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      return;
    }
    // the window's own shortcuts wait while the library is open
    if (event.ctrlKey || event.metaKey) event.stopPropagation();
  }

  onMount(() => {
    view = initialView === "missing" && documentFonts.length ? "missing" : "all";
    const stopFollowing = store.follow();
    const releaseDrop = setFileDropTarget({
      over: () => {
        dropping = true;
        return true;
      },
      drop: (paths) => {
        dropping = false;
        const fonts = fontFilesAmong(paths);
        if (!fonts.length) {
          announcement = "Only font files can be dropped here: .ttf, .otf, .ttc, .woff or .woff2.";
          return true;
        }
        void addPaths(fonts);
        return true;
      },
      leave: () => {
        dropping = false;
      },
    });
    const resize = new ResizeObserver(() => {
      if (list) viewport = list.clientHeight;
    });
    if (list) resize.observe(list);
    searchField?.focus();
    return () => {
      stopFollowing();
      releaseDrop();
      resize.disconnect();
    };
  });
</script>

<div class="backdrop" data-page-ui>
  <div
    class="library"
    role="dialog"
    aria-modal="true"
    aria-labelledby="font-library-title"
    tabindex="-1"
    onkeydown={onKeyDown}
  >
    <header>
      <h2 id="font-library-title">{tools.fontLibrary.label}</h2>
      {#if documentFonts.length}
        <div class="segments" role="tablist" aria-label="Font Library view">
          <button role="tab" aria-selected={view === "all"} class:chosen={view === "all"} onclick={() => (view = "all")}>All Fonts</button>
          <button role="tab" aria-selected={view === "missing"} class:chosen={view === "missing"} onclick={() => (view = "missing")}>
            Missing Fonts{missing.length ? ` (${missing.length})` : ""}
          </button>
        </div>
      {/if}
      <div class="trailing">
        <button class="button" title={tooltipFor("addFonts")} onclick={addFiles}>{tools.addFonts.label}</button>
        <button class="button" title={tooltipFor("addFontFolder")} onclick={addFolder}>{tools.addFontFolder.label}</button>
        <button class="button" onclick={onClose}>Done</button>
      </div>
    </header>

    {#if view === "all"}
      <div class="controls">
        <label class="search">
          <Icon name="search" size={1} />
          <span class="visually-hidden">Search fonts</span>
          <input
            bind:this={searchField}
            type="search"
            placeholder="Search by name, kind or language"
            autocomplete="off"
            spellcheck="false"
            aria-controls="font-library-list"
            aria-describedby="font-library-count"
            bind:value={filters.text}
            onkeydown={onSearchKey}
          />
        </label>

        <div class="chips" role="group" aria-label="Kind of font">
          {#each CATEGORY_CHIPS as category (category)}
            <button
              class="chip"
              aria-pressed={filters.category === category}
              onclick={() => (filters.category = toggle(filters.category, category))}>{CATEGORY_LABELS[category]}</button
            >
          {/each}
          <select class="chip select" aria-label="Language" bind:value={filters.writing}>
            <option value={null}>Any Language</option>
            {#each store.writing as writing (writing.id)}
              <option value={writing.id}>{writing.name}</option>
            {/each}
          </select>
        </div>

        <div class="chips" role="group" aria-label="Where fonts come from">
          {#each PLACE_CHIPS as place (place)}
            <button class="chip" aria-pressed={filters.place === place} onclick={() => (filters.place = toggle(filters.place, place))}
              >{PLACE_LABELS[place]}</button
            >
          {/each}
          <button
            class="chip"
            aria-pressed={filters.embeddable}
            title="Only fonts whose license allows saving them inside PDFs and documents"
            onclick={() => (filters.embeddable = !filters.embeddable)}>Embeddable</button
          >
          <button class="chip" aria-pressed={filters.favouritesOnly} onclick={() => (filters.favouritesOnly = !filters.favouritesOnly)}
            >Favourites</button
          >
        </div>

        <div class="sample-row">
          <label class="sample-field">
            <span class="visually-hidden">Sample text</span>
            <input class="field" type="text" placeholder="Type sample text" bind:value={filters.sample} />
          </label>
          <label class="check" title="Hide fonts that can’t draw every character of the sample">
            <input type="checkbox" bind:checked={filters.mustDrawSample} />
            <span>Can Draw It</span>
          </label>
          <label class="size">
            <span class="visually-hidden">Preview size</span>
            <input
              type="range"
              min={SIZE_RANGE.min}
              max={SIZE_RANGE.max}
              value={size}
              oninput={(event) => (size = clampSize(Number((event.target as HTMLInputElement).value)))}
            />
            <output>{size} pt</output>
          </label>
        </div>

        <p id="font-library-count" class="count" aria-live="polite">
          {countSentence}
          {#if results.understood.length}<span class="understood">{results.understood.join(" · ")}</span>{/if}
        </p>
      </div>

      <div class="body">
        <div
          id="font-library-list"
          class="list"
          role="listbox"
          tabindex="0"
          aria-label="Fonts"
          aria-activedescendant={selected >= 0 ? `font-row-${selected}` : undefined}
          bind:this={list}
          onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
          onkeydown={onListKey}
        >
          <div class="spacer" style:height="{rows.total}px">
            <div class="window" style:transform="translateY({rows.offset}px)">
              {#each shown.slice(rows.start, rows.end) as family, offset (family.name)}
                {@const index = rows.start + offset}
                {@const download = store.status?.downloads.find((entry) => entry.family === family.name && !entry.finished && !entry.error)}
                <div
                  id="font-row-{index}"
                  class="row"
                  class:selected={index === selected}
                  role="option"
                  aria-selected={index === selected}
                  aria-setsize={shown.length}
                  aria-posinset={index + 1}
                  style:height="{rowHeight}px"
                  tabindex="-1"
                  onclick={() => select(index, false)}
                  onkeydown={() => {}}
                >
                  <div class="name-line">
                    <span class="name">{family.name}</span>
                    {#if family.favourite}<span class="star" aria-label="Favourite">★</span>{/if}
                    {#if metricNames.has(family.name)}<span class="badge">Same Widths</span>{/if}
                    <span class="meta">
                      {download ? `Downloading… ${sizeWords(download.progress.done)} of ${sizeWords(download.progress.total)}` : familyMeta(family)}
                    </span>
                  </div>
                  <FontSample {family} text={sampleFor(family.name, filters.sample, filters.writing)} {size} />
                </div>
              {/each}
            </div>
          </div>
          {#if shown.length === 0}
            <p class="empty">{countSentence}</p>
          {/if}
        </div>

        <FontDetail
          family={selectedFamily}
          sample={filters.sample}
          writing={filters.writing}
          {size}
          onShowFamily={showFamily}
          onAnnounce={(text) => (announcement = text)}
        />
      </div>
    {:else}
      <MissingFonts fonts={missing} {onReplaceFont} onAnnounce={(text) => (announcement = text)} onShowFamily={(name) => {
        view = "all";
        showFamily(name);
      }} />
    {/if}

    <p class="visually-hidden" aria-live="polite">{announcement}</p>
    {#if announcement}
      <p class="announcement">{announcement}</p>
    {/if}
    {#if dropping}
      <div class="drop" aria-hidden="true"><p>Drop font files to add them to Letters</p></div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: flex;
    justify-content: center;
    align-items: stretch;
    padding: 1rem;
    background: rgb(0 0 0 / 0.16);
  }

  .library {
    position: relative;
    display: flex;
    flex-direction: column;
    width: min(64rem, 100%);
    min-height: 0;
    border-radius: var(--radius-group);
    background: var(--window);
    box-shadow: var(--shadow-sheet);
    outline: none;
    overflow: hidden;
    animation: rise 0.2s var(--ease);
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.875rem 1rem 0.5rem;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .trailing {
    display: flex;
    gap: 0.5rem;
    margin-left: auto;
  }

  .segments {
    display: flex;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segments button {
    padding: 0.1875rem 0.75rem;
    border: 0;
    border-radius: var(--radius-small);
    background: none;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
  }

  .segments button.chosen {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }

  .controls {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0 1rem 0.5rem;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    padding: 0.375rem 0.625rem;
    border-radius: var(--radius-control);
    background: var(--control);
    color: var(--label-secondary);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: none;
    color: var(--label);
    font: var(--body);
  }

  /* the ring goes round the whole capsule, not the bare field inside it */
  .search input:focus-visible {
    outline: none;
    box-shadow: none;
  }

  .search:focus-within {
    outline: 3px solid var(--accent-soft);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }

  .chip {
    padding: 0.1875rem 0.625rem;
    border: 0;
    border-radius: 1rem;
    background: var(--control);
    color: var(--label);
    font: var(--subheadline);
    font-weight: 500;
    letter-spacing: var(--subheadline-spacing);
  }

  .chip:hover {
    background: var(--control-hover);
  }

  /* an on chip uses the accent: it is a toggle's on state */
  .chip[aria-pressed="true"] {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .select {
    appearance: auto;
  }

  .sample-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .sample-field {
    flex: 1;
    display: flex;
  }

  .sample-field input {
    flex: 1;
    min-width: 0;
  }

  .size {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
  }

  .size input {
    width: 7rem;
    accent-color: var(--accent);
  }

  .size output {
    min-width: 2.75rem;
  }

  .count {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .understood {
    margin-left: 0.5rem;
    color: var(--label);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
    border-top: 0.5px solid var(--separator);
  }

  .list {
    position: relative;
    flex: 1.4;
    min-width: 0;
    overflow-y: auto;
    outline: none;
    contain: strict;
  }

  .list:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent-soft);
  }

  .spacer {
    position: relative;
  }

  .window {
    position: absolute;
    inset: 0 0 auto;
    will-change: transform;
  }

  .row {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.125rem;
    padding: 0.375rem 1rem;
    border-bottom: 0.5px solid var(--separator);
    overflow: hidden;
  }

  .row.selected {
    background: var(--accent-soft);
  }

  .name-line {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    min-width: 0;
  }

  .name {
    font: var(--headline);
    white-space: nowrap;
  }

  .star {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .badge {
    padding: 0 0.375rem;
    border-radius: 0.5rem;
    background: var(--control);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
  }

  .meta {
    overflow: hidden;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    padding: 2rem 1rem;
    color: var(--label-secondary);
    text-align: center;
  }

  .announcement {
    margin: 0;
    padding: 0.5rem 1rem;
    border-top: 0.5px solid var(--separator);
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .drop {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    border: 2px dashed var(--accent);
    border-radius: var(--radius-group);
    background: rgb(0 0 0 / 0.04);
    pointer-events: none;
  }

  .drop p {
    padding: 0.75rem 1.25rem;
    border-radius: var(--radius-control);
    background: var(--window);
    box-shadow: var(--shadow-sheet);
    font: var(--headline);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(0.75rem);
    }
  }

  @media (max-width: 46rem) {
    .backdrop {
      padding: 0;
    }
    .body {
      flex-direction: column;
    }
    .trailing .button {
      padding: 0.3125rem 0.5rem;
    }
  }
</style>
