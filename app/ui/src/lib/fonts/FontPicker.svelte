<script lang="ts">
  // a compact font menu for the writer and the PDF text tools.
  //
  //   <FontPicker value={font} documentFonts={used} onChange={(family) => setFont(family)} />
  //
  // pressing it opens a searchable list: Favourites, Recent, In This
  // Document, then All Fonts on this computer, each name drawn in its own
  // font. typing filters as you go (typos forgiven); the arrows, Page Up
  // and Down, Home and End move, Enter chooses, Escape closes. More Fonts…
  // opens the Font Library. the list is virtual, so a thousand fonts open
  // as quickly as ten. a font whose license bars embedding gets a quiet
  // note under the button.
  import { onMount, tick } from "svelte";
  import { tools, tooltipFor } from "../tools";
  import { cssFamily, ensurePreviewFace, useFontInDocument } from "./fontFaces";
  import { fontsApi } from "./fontsApi";
  import { latestOnly } from "./fontQuery";
  import { fontStore } from "./fontStore.svelte";
  import FontSample from "./FontSample.svelte";
  import { openFontLibrary } from "./openFontLibrary";
  import { embeddingNote, groupedRows, indexOfFamily, isFamilyRow, matchRows, type PickerRow } from "./pickerRows";
  import { moveWithKey, rowsPerPage, scrollToShow, visibleRows } from "./virtualList";

  interface Props {
    /** the chosen family's name */
    value: string;
    /** called with the family the person chose */
    onChange: (family: string) => void;
    /** fonts the open document uses, listed under In This Document */
    documentFonts?: string[];
    /** read out to screen readers; shown as the tooltip */
    label?: string;
    disabled?: boolean;
  }

  let { value, onChange, documentFonts = [], label = tools.fontPicker.label, disabled = false }: Props = $props();

  const ROW = 28;
  const VIEWPORT = 320;
  const id = `font-picker-${Math.random().toString(36).slice(2, 8)}`;
  const store = fontStore();

  let opened = $state(false);
  let text = $state("");
  let matches = $state<number[] | null>(null);
  let active = $state(-1);
  let scrollTop = $state(0);
  let list = $state<HTMLDivElement>();
  let field = $state<HTMLInputElement>();
  let button = $state<HTMLButtonElement>();
  let root = $state<HTMLDivElement>();

  const current = $derived(store.family(value) ?? null);
  const note = $derived(embeddingNote(current));
  const rows = $derived<PickerRow[]>(
    matches === null ? groupedRows(store.families, store.personal, documentFonts) : matchRows(store.families, matches),
  );
  const visible = $derived(visibleRows(scrollTop, VIEWPORT, ROW, rows.length));

  const runSearch = latestOnly(fontsApi.search);

  // the chosen font's own face, for the button
  $effect(() => {
    if (current?.needsFontFace) void ensurePreviewFace(current);
  });

  $effect(() => {
    const typed = text.trim();
    void store.revision;
    if (!typed) {
      matches = null;
      return;
    }
    void runSearch({ text: typed, places: ["onThisComputer", "lettersLibrary", "addedByMe"] }).then(async (answer) => {
      if (!answer.current) return;
      if (answer.value.revision !== store.revision) await store.loadFamilies();
      matches = answer.value.results.families;
      active = rows.findIndex(isFamilyRow);
    });
  });

  async function show() {
    if (disabled) return;
    opened = true;
    text = "";
    matches = null;
    await tick();
    field?.focus();
    const at = indexOfFamily(rows, value);
    active = at >= 0 ? at : rows.findIndex(isFamilyRow);
    if (list && active >= 0) {
      list.scrollTop = Math.max(0, active * ROW - VIEWPORT / 2);
      scrollTop = list.scrollTop;
    }
  }

  function hide(focusButton = true) {
    opened = false;
    if (focusButton) button?.focus();
  }

  async function choose(name: string) {
    hide();
    // a font that isn't installed is given to the web view before use
    const family = store.family(name);
    if (family?.needsFontFace) await useFontInDocument(name);
    void store.noteUsed(name);
    onChange(name);
  }

  function moreFonts() {
    hide(false);
    openFontLibrary({ documentFonts, family: value || undefined });
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      hide();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const row = rows[active];
      if (isFamilyRow(row)) void choose(row.name);
      return;
    }
    // inside the search field Home and End move the caret, not the list
    if ((event.key === "Home" || event.key === "End") && event.target === field) return;
    const next = moveWithKey(event.key, active, rows.length, rowsPerPage(VIEWPORT, ROW), (index) => isFamilyRow(rows[index]));
    if (next === null) return;
    event.preventDefault();
    active = next;
    if (list) {
      list.scrollTop = scrollToShow(next, list.scrollTop, VIEWPORT, ROW);
      scrollTop = list.scrollTop;
    }
  }

  onMount(() => {
    const stop = store.follow();
    const outside = (event: PointerEvent) => {
      if (opened && root && !root.contains(event.target as Node)) hide(false);
    };
    window.addEventListener("pointerdown", outside, true);
    return () => {
      stop();
      window.removeEventListener("pointerdown", outside, true);
    };
  });
</script>

<div class="picker" bind:this={root} data-page-ui>
  <button
    bind:this={button}
    class="current"
    type="button"
    {disabled}
    aria-haspopup="listbox"
    aria-expanded={opened}
    aria-label="{label}: {value || 'none'}"
    title={tooltipFor("fontPicker")}
    style:font-family={current?.onThisComputer ? cssFamily(current.name) : undefined}
    onclick={() => (opened ? hide() : show())}
  >
    <span class="value">{value || "Choose a Font"}</span>
    <svg class="chevron" viewBox="0 0 24 24" aria-hidden="true"><path d="M6.5 9.5 12 15l5.5-5.5" /></svg>
  </button>
  {#if note}
    <p class="note">{note}</p>
  {/if}

  {#if opened}
    <div class="popover glass" role="presentation" onkeydown={onKey}>
      <input
        bind:this={field}
        class="field search"
        type="search"
        placeholder="Search fonts"
        autocomplete="off"
        spellcheck="false"
        role="combobox"
        aria-expanded="true"
        aria-controls="{id}-list"
        aria-activedescendant={active >= 0 ? `${id}-row-${active}` : undefined}
        aria-label="Search fonts"
        bind:value={text}
      />
      <div
        id="{id}-list"
        class="list"
        role="listbox"
        aria-label={label}
        tabindex="-1"
        bind:this={list}
        style:height="{Math.min(VIEWPORT, Math.max(ROW, rows.length * ROW))}px"
        onscroll={() => (scrollTop = list?.scrollTop ?? 0)}
      >
        <div style:height="{visible.total}px" class="spacer">
          <div class="window" style:transform="translateY({visible.offset}px)">
            {#each rows.slice(visible.start, visible.end) as row, offset (visible.start + offset)}
              {@const index = visible.start + offset}
              {#if row.kind === "heading"}
                <div class="heading" role="presentation" style:height="{ROW}px">{row.label}</div>
              {:else}
                <div
                  id="{id}-row-{index}"
                  class="row"
                  class:active={index === active}
                  class:missing={!row.family?.onThisComputer}
                  role="option"
                  aria-selected={row.name === value}
                  aria-label={row.family?.onThisComputer ? row.name : `${row.name}, not on this computer`}
                  tabindex="-1"
                  style:height="{ROW}px"
                  onpointerenter={() => (active = index)}
                  onclick={() => choose(row.name)}
                  onkeydown={() => {}}
                >
                  <FontSample family={row.family} name={row.name} text={row.name} size={11} />
                  {#if row.name === value}<span class="tick" aria-hidden="true">✓</span>{/if}
                </div>
              {/if}
            {/each}
          </div>
        </div>
        {#if rows.length === 0}
          <p class="empty">No fonts on this computer match “{text.trim()}”. More Fonts… can find it in the library.</p>
        {/if}
      </div>
      <div class="footer">
        <button class="button" type="button" title={tooltipFor("moreFonts")} onclick={moreFonts}>{tools.moreFonts.label}</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    min-width: 0;
  }

  .current {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.375rem;
    width: 100%;
    padding: 0.3125rem 0.5rem 0.3125rem 0.625rem;
    border: 0;
    border-radius: var(--radius-small);
    background: var(--control);
    font-size: 0.8125rem;
    text-align: left;
  }

  .current:hover {
    background: var(--control-hover);
  }

  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chevron {
    flex: none;
    width: 0.875rem;
    height: 0.875rem;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
    color: var(--label-secondary);
  }

  .note {
    margin: 0.25rem 0 0;
    color: var(--label-secondary);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
  }

  .popover {
    position: absolute;
    z-index: 30;
    top: calc(100% + 0.25rem);
    left: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    width: max(100%, 16rem);
    padding: var(--group-padding);
    background: var(--window);
  }

  .search {
    margin: 0.125rem;
  }

  .list {
    position: relative;
    overflow-y: auto;
    contain: strict;
  }

  .spacer {
    position: relative;
  }

  .window {
    position: absolute;
    inset: 0 0 auto;
  }

  .heading {
    display: flex;
    align-items: flex-end;
    padding: 0 0.5rem 0.25rem;
    color: var(--label-secondary);
    font: var(--footnote);
    font-weight: 600;
    letter-spacing: var(--footnote-spacing);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0.5rem;
    border-radius: var(--radius-control);
  }

  /* the row under the keys or the pointer is the selection, so it uses the accent */
  .row.active {
    background: var(--accent);
    color: var(--on-accent);
  }

  .row.active :global(.sample) {
    color: var(--on-accent);
  }

  .row.missing :global(.sample) {
    font-style: italic;
  }

  .tick {
    font: var(--subheadline);
  }

  .empty {
    margin: 0.5rem;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .footer {
    display: flex;
    justify-content: flex-end;
    padding: 0.25rem;
    border-top: 0.5px solid var(--separator);
  }
</style>
