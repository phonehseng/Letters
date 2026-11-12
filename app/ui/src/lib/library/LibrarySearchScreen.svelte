<script lang="ts">
  // Search All Documents: one field, results as you type, grouped by
  // document with their best pages, filter chips, and the keyboard for
  // everything. Enter opens the document at that page with the words
  // highlighted by its own Find bar.
  //
  // the rules (rows, keys, sentences) are in results.ts; this file only
  // draws them and keeps the newest query's answer.
  import { onMount, untrack } from "svelte";
  import Icon from "../Icon.svelte";
  import { tools, tooltipFor } from "../tools";
  import { findTextFor } from "./arrival";
  import LibrarySettingsPanel from "./LibrarySettingsPanel.svelte";
  import { libraryOverview, refreshLibrary, searchLibrary, type DocKind, type DocumentHit, type LibraryOverview, type PassageHit, type SearchResults } from "./libraryApi";
  import {
    correctionSentence,
    countSentence,
    DATE_LABELS,
    emptyState,
    filtersOf,
    folderOf,
    KIND_LABELS,
    noteSentence,
    passagePlace,
    rowsOf,
    snippetPieces,
    statusSentence,
    stepDocument,
    stepRow,
    stillIndexing,
    targetOf,
    toggleKind,
    type DateChip,
    type ResultRow,
  } from "./results";

  interface Props {
    initialQuery?: string;
    onClose: () => void;
    /** opens a result: here (or in a new window when asked) at the passage */
    onOpen: (hit: DocumentHit, passage: PassageHit | null, query: string, newWindow: boolean) => void;
  }

  let { initialQuery = "", onClose, onOpen }: Props = $props();

  let field = $state<HTMLInputElement>();
  let query = $state(untrack(() => initialQuery));
  let kinds = $state<DocKind[]>([]);
  let date = $state<DateChip>("any");
  let titlesOnly = $state(false);
  let folder = $state<string | null>(null);
  let results = $state<SearchResults | null>(null);
  let failed = $state<string | null>(null);
  let current = $state(-1);
  let overview = $state<LibraryOverview | null>(null);
  let showSettings = $state(false);
  let list = $state<HTMLUListElement>();

  const rows = $derived(rowsOf(results));
  const status = $derived(overview?.status ?? null);
  const hasSources = $derived(!!overview && (overview.settings.libraryFolders.length > 0 || overview.settings.searchedPinnedFolders.length > 0 || overview.settings.includeRecents));
  const empty = $derived(emptyState(query, results, status, hasSources));
  const counted = $derived(countSentence(results, query));
  const corrected = $derived(correctionSentence(results));
  const folders = $derived(overview ? [...overview.settings.libraryFolders, ...overview.settings.searchedPinnedFolders] : []);

  // only the newest query's answer is shown: a slow answer to "pho" must
  // not replace the answer to "photosynthesis"
  let asked = 0;
  async function runSearch() {
    const ticket = ++asked;
    const text = query;
    if (!text.trim()) {
      results = null;
      current = -1;
      return;
    }
    try {
      const found = await searchLibrary({
        query: text,
        filters: filtersOf(kinds, date, titlesOnly, folder, Date.now()),
        maxDocuments: 40,
        passagesPerDocument: 3,
      });
      if (ticket !== asked) return;
      failed = null;
      results = found;
      current = found.documents.length ? 0 : -1;
    } catch (problem) {
      if (ticket !== asked) return;
      failed = (problem as { message?: string })?.message ?? "Search failed.";
    }
  }

  // every change of the query or a chip searches again
  $effect(() => {
    void [query, kinds, date, titlesOnly, folder];
    untrack(runSearch);
  });

  async function loadOverview() {
    try {
      overview = await libraryOverview();
    } catch {
      overview = null;
    }
  }

  // the job's progress, refreshed while the screen shows; results are
  // asked for again while documents are still being indexed
  onMount(() => {
    field?.focus();
    field?.select();
    refreshLibrary().catch(() => {});
    loadOverview();
    const timer = setInterval(async () => {
      const wasIndexing = stillIndexing(status);
      await loadOverview();
      if (wasIndexing && query.trim()) runSearch();
    }, 1500);
    return () => clearInterval(timer);
  });

  function open(row: ResultRow | undefined, newWindow: boolean) {
    if (!results || !row) return;
    const { hit, passage } = targetOf(results, row);
    // the document's Find bar takes plain words, not the search syntax
    onOpen(hit, passage, findTextFor(query, results.corrections), newWindow);
  }

  function select(index: number) {
    current = index;
    // the chosen row stays in view; the list is the one scrolling part
    list?.querySelectorAll<HTMLElement>("[data-row]")[index]?.scrollIntoView({ block: "nearest" });
  }

  function onKeyDown(event: KeyboardEvent) {
    const newWindow = event.ctrlKey || event.metaKey;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      select(stepRow(current, rows.length, event.key === "ArrowDown" ? 1 : -1));
    } else if (event.key === "PageDown" || event.key === "PageUp") {
      event.preventDefault();
      select(stepDocument(rows, current, event.key === "PageDown" ? 1 : -1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      open(rows[current], newWindow);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (showSettings) showSettings = false;
      else if (query) query = "";
      else onClose();
    }
  }

  function rowIndex(document: number, passage: number | null): number {
    return rows.findIndex((row) => row.document === document && (passage === null ? row.kind === "document" : row.kind === "passage" && row.passage === passage));
  }

  function kindIcon(kind: DocKind): string {
    return kind === "pdf" ? "PDF" : kind === "word" ? "DOC" : "TXT";
  }

  function modifiedText(milliseconds: number): string {
    if (!milliseconds) return "";
    return new Date(milliseconds).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="library" onkeydown={onKeyDown}>
  <header class="toolbar">
    <div class="glass group">
      <button class="tool" onclick={onClose} title="Back (Esc)" aria-label="Back">
        <Icon name="back" size={1.0625} />
      </button>
    </div>
    <h1>Search All Documents</h1>
    <div class="glass group">
      <button class="tool labelled" class:pressed={showSettings} aria-pressed={showSettings} onclick={() => (showSettings = !showSettings)} title={tooltipFor("librarySettings")}>
        <Icon name="tools" size={1} />
        <span>Settings</span>
      </button>
    </div>
  </header>

  <main>
    <div class="glass search" role="search">
      <Icon name="search" size={1.0625} />
      <input
        bind:this={field}
        bind:value={query}
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="Search all documents"
        aria-label="Search all documents"
        aria-controls="library-results"
        aria-activedescendant={current >= 0 ? `library-row-${current}` : undefined}
      />
      <span class="count" aria-live="polite">{counted}</span>
    </div>

    <div class="chips" role="group" aria-label="Filters">
      {#each Object.entries(KIND_LABELS) as [kind, label] (kind)}
        <button class="chip" class:on={kinds.includes(kind as DocKind)} aria-pressed={kinds.includes(kind as DocKind)} onclick={() => (kinds = toggleKind(kinds, kind as DocKind))}>{label}</button>
      {/each}
      <label class="chip select">
        <span class="visually-hidden">Changed</span>
        <select bind:value={date} aria-label="Changed">
          {#each Object.entries(DATE_LABELS) as [value, label] (value)}
            <option {value}>{label}</option>
          {/each}
        </select>
      </label>
      {#if folders.length > 0}
        <label class="chip select">
          <span class="visually-hidden">Folder</span>
          <select bind:value={folder} aria-label="Folder">
            <option value={null}>All Folders</option>
            {#each folders as each (each)}
              <option value={each}>{each.split(/[\\/]/).filter(Boolean).at(-1)}</option>
            {/each}
          </select>
        </label>
      {/if}
      <button class="chip" class:on={titlesOnly} aria-pressed={titlesOnly} title={tooltipFor("titlesOnly")} onclick={() => (titlesOnly = !titlesOnly)}>{tools.titlesOnly.label}</button>
    </div>

    <p class="status" aria-live="polite">
      {statusSentence(status)}
      {#if status?.phase === "indexing" && status.current}
        <span class="current">{status.current.split(/[\\/]/).at(-1)}</span>
      {/if}
      <span class="local">The index is kept on this computer. Nothing is sent anywhere.</span>
    </p>

    {#if corrected}
      <p class="corrected">{corrected}</p>
    {/if}
    {#if failed}
      <p class="corrected" role="alert">{failed}</p>
    {/if}

    {#if empty}
      <div class="empty">
        <h2>{empty.title}</h2>
        <p>{empty.detail}</p>
        {#if !hasSources || (results && results.totalDocuments === 0 && !stillIndexing(status))}
          <button class="button" onclick={() => (showSettings = true)}>{tools.addLibraryFolder.label}</button>
        {/if}
      </div>
    {:else if results}
      <ul class="results" id="library-results" role="listbox" aria-label="Results" bind:this={list}>
        {#each results.documents as hit, d (hit.path)}
          {@const docRow = rowIndex(d, null)}
          <li class="document" role="presentation">
            <div
              class="row head"
              class:current={current === docRow}
              role="option"
              id="library-row-{docRow}"
              aria-selected={current === docRow}
              tabindex="-1"
              data-row
              onclick={() => {
                current = docRow;
                open(rows[docRow], false);
              }}
              onkeydown={() => {}}
            >
              <span class="kind" aria-hidden="true">{kindIcon(hit.kind)}</span>
              <span class="title-block">
                <span class="title">{hit.title}</span>
                <span class="path">{folderOf(hit.path)} · {modifiedText(hit.modified)}{hit.matchingPassages > 1 ? ` · ${hit.matchingPassages} places` : ""}</span>
                {#each hit.notes as note, n (n)}
                  <span class="note">{noteSentence(note)}</span>
                {/each}
              </span>
              <button
                class="new-window"
                title={tooltipFor("openInNewWindow")}
                onclick={(event) => {
                  event.stopPropagation();
                  open(rows[docRow], true);
                }}>{tools.openInNewWindow.label}</button
              >
            </div>
            {#each hit.passages as passage, p (p)}
              {@const passageRow = rowIndex(d, p)}
              {#if passage.snippet}
                <div
                  class="row passage"
                  class:current={current === passageRow}
                  role="option"
                  id="library-row-{passageRow}"
                  aria-selected={current === passageRow}
                  tabindex="-1"
                  data-row
                  onclick={() => {
                    current = passageRow;
                    open(rows[passageRow], false);
                  }}
                  onkeydown={() => {}}
                >
                  <span class="place">{passagePlace(hit.kind, passage)}</span>
                  <span class="snippet">
                    {#each snippetPieces(passage.snippet, passage.highlights) as piece, i (i)}
                      {#if piece.marked}<mark>{piece.text}</mark>{:else}{piece.text}{/if}
                    {/each}
                  </span>
                </div>
              {/if}
            {/each}
          </li>
        {/each}
      </ul>
      {#if results.totalDocuments > results.documents.length}
        <p class="more">Showing the best {results.documents.length} of {results.totalDocuments} documents. Add words to narrow the search.</p>
      {/if}
    {/if}
  </main>

  {#if showSettings}
    <LibrarySettingsPanel onClose={() => (showSettings = false)} onChanged={(next) => (overview = next)} />
  {/if}
</div>

<style>
  .library {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--window);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.625rem 0.75rem;
  }

  h1 {
    flex: 1;
    margin: 0;
    font: var(--headline);
  }

  .group {
    display: flex;
    padding: var(--group-padding);
  }

  .tool {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    height: 2rem;
    min-width: 2rem;
    justify-content: center;
    padding: 0 0.5rem;
    border: 0;
    border-radius: calc(var(--radius-group) - 4px);
    background: transparent;
    color: var(--label);
    font: var(--body);
  }

  .tool:hover {
    background: var(--control-hover);
  }

  .tool.pressed {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  main {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0.5rem max(1.5rem, calc(50% - 26rem)) 3rem;
  }

  /* the one prominent control: a wide capsule */
  .search {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    height: 2.75rem;
    padding: 0 1rem;
    border-radius: 1.375rem;
    color: var(--label-secondary);
  }

  .search:focus-within {
    box-shadow:
      0 0 0 1px var(--accent),
      0 0 0 4px var(--accent-soft),
      var(--shadow-glass);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    /* the capsule shows the focus; the field inside must not draw a second one */
    box-shadow: none;
    background: transparent;
    color: var(--label);
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .count {
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
    margin: 0.75rem 0 0.5rem;
  }

  .chip {
    height: 1.75rem;
    padding: 0 0.75rem;
    border: 0;
    border-radius: 0.875rem;
    background: var(--control);
    color: var(--label);
    font: var(--subheadline);
  }

  .chip.on {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .chip.select {
    display: flex;
    align-items: center;
    padding: 0 0.25rem;
  }

  .chip select {
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
  }

  .status,
  .corrected,
  .more {
    margin: 0.25rem 0 0.75rem;
    color: var(--label-secondary);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
  }

  .status .current {
    margin-left: 0.25rem;
    font-style: italic;
  }

  .status .local {
    display: block;
  }

  .empty {
    margin-top: 4rem;
    text-align: center;
  }

  .empty h2 {
    margin: 0 0 0.5rem;
    font: var(--title);
  }

  .empty p {
    max-width: 26rem;
    margin: 0 auto 1rem;
    color: var(--label-secondary);
  }

  .results {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .document {
    margin-bottom: 1rem;
  }

  .row {
    display: flex;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    border-radius: var(--radius-small);
    cursor: default;
  }

  .row:hover {
    background: var(--control-hover);
  }

  .row.current {
    background: var(--accent-soft);
  }

  .head {
    align-items: center;
  }

  .kind {
    display: grid;
    place-items: center;
    flex: none;
    width: 2.25rem;
    height: 2.75rem;
    border-radius: 3px;
    background: #ffffff;
    box-shadow: var(--shadow-sheet);
    color: var(--label-secondary);
    font: var(--footnote);
    font-weight: 600;
  }

  .title-block {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
  }

  .title {
    font: var(--headline);
  }

  .path,
  .note {
    overflow: hidden;
    color: var(--label-secondary);
    font: var(--subheadline);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .new-window {
    flex: none;
    height: 1.75rem;
    padding: 0 0.625rem;
    border: 0;
    border-radius: 0.875rem;
    background: transparent;
    color: var(--accent-label);
    font: var(--subheadline);
    opacity: 0;
  }

  .row:hover .new-window,
  .row.current .new-window,
  .new-window:focus-visible {
    opacity: 1;
  }

  .passage {
    flex-direction: column;
    gap: 0.125rem;
    margin-left: 3rem;
  }

  .place {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .snippet {
    font: var(--body);
    line-height: 1.45;
  }

  /* highlighted words: the system's own highlight, not the accent */
  mark {
    border-radius: 2px;
    background: rgba(255, 214, 10, 0.45);
    color: inherit;
  }
</style>
