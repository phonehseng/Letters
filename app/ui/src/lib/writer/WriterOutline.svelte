<script lang="ts">
  // the outline in the sidebar: every heading, indented by level, updated
  // as you type. click to go there; the arrow buttons (or Alt Left and
  // Alt Right) promote and demote; drag a heading, or press Alt Up and
  // Alt Down, to move its whole section.
  import { tick } from "svelte";
  import { countWords, sectionCounts, wordsLabel } from "../longform/manuscript";
  import { tools, tooltipFor } from "../tools";
  import { currentEntry, sectionBlocks, type OutlineEntry } from "./outline";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
  }

  let { session }: Props = $props();

  const entries = $derived(session.outline());
  const current = $derived(currentEntry(entries, session.editor.selection.from));
  // each section's words beside its heading, as a manuscript navigator shows them (lib/longform)
  const sectionWords = $derived.by(() => {
    const blockWords: number[] = [];
    session.editor.doc.forEach((block) => blockWords.push(countWords(block.textContent)));
    const counts = sectionCounts(entries.map((entry) => ({ blockIndex: entry.index, level: entry.level, text: entry.text })), blockWords);
    return new Map(counts.map((count) => [count.blockIndex, count.words]));
  });
  let dragged = $state<OutlineEntry | null>(null);
  let dropBefore = $state<number | null>(null);
  let list = $state<HTMLOListElement>();

  async function refocus(index: number) {
    await tick();
    list?.querySelectorAll<HTMLButtonElement>(".entry")[index]?.focus();
  }

  function onKey(entry: OutlineEntry, position: number, event: KeyboardEvent) {
    if (!event.altKey) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        refocus(Math.min(Math.max(position + (event.key === "ArrowDown" ? 1 : -1), 0), entries.length - 1));
      }
      return;
    }
    if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      event.preventDefault();
      session.shiftHeading(entry, event.key === "ArrowLeft" ? "promote" : "demote");
      refocus(position);
    } else if (event.key === "ArrowUp" && position > 0) {
      event.preventDefault();
      session.moveSection(entry, entries[position - 1].index);
      refocus(position - 1);
    } else if (event.key === "ArrowDown" && position < entries.length - 1) {
      event.preventDefault();
      const after = sectionBlocks(session.editor.doc, entries[position + 1].index).end;
      session.moveSection(entry, after);
      refocus(position + 1);
    }
  }

  function onDrop(event: DragEvent) {
    event.preventDefault();
    if (dragged && dropBefore !== null) session.moveSection(dragged, dropBefore);
    dragged = null;
    dropBefore = null;
  }
</script>

<aside class="sidebar" aria-label="Outline">
  <header>
    <h2>{tools.wOutline.label}</h2>
  </header>
  {#if entries.length === 0}
    <p class="empty">No headings yet. Choose a heading style from Style in the toolbar, or press Ctrl Alt 1.</p>
  {:else}
    <ol class="entries" bind:this={list} ondragover={(event) => event.preventDefault()} ondrop={onDrop}>
      {#each entries as entry, position (entry.pos)}
        <li
          class:drop-before={dropBefore === entry.index && dragged !== entry}
          style:padding-left="{(entry.level - 1) * 0.75}rem"
          ondragenter={() => (dropBefore = entry.index)}
        >
          <button
            class="entry"
            class:current={current?.pos === entry.pos}
            draggable="true"
            title={`${entry.text || "Untitled heading"}. ${tooltipFor("wOutline")}`}
            aria-current={current?.pos === entry.pos ? "location" : undefined}
            aria-label={`${entry.text || "Untitled heading"}, heading ${entry.level}`}
            ondragstart={(event) => {
              dragged = entry;
              event.dataTransfer?.setData("text/plain", entry.text);
            }}
            ondragend={() => ((dragged = null), (dropBefore = null))}
            onclick={() => session.goToHeading(entry)}
            onkeydown={(event) => onKey(entry, position, event)}
          >
            <span class="text">{entry.text || "Untitled heading"}</span>
            <span class="words">{wordsLabel(sectionWords.get(entry.index) ?? 0)}</span>
          </button>
          <span class="shift">
            <button title={tooltipFor("wPromote")} aria-label={`Promote ${entry.text}`} disabled={entry.level === 1} onclick={() => session.shiftHeading(entry, "promote")}>‹</button>
            <button title={tooltipFor("wDemote")} aria-label={`Demote ${entry.text}`} disabled={entry.level === 6} onclick={() => session.shiftHeading(entry, "demote")}>›</button>
          </span>
        </li>
      {/each}
      <li
        class="end"
        class:drop-before={dropBefore === session.editor.doc.childCount}
        ondragenter={() => (dropBefore = session.editor.doc.childCount)}
      ></li>
    </ol>
  {/if}
</aside>

<style>
  .words {
    flex-shrink: 0;
    margin-left: auto;
    padding-left: 0.375rem;
    color: var(--label-secondary);
    font: var(--footnote);
  }

  .sidebar {
    flex-shrink: 0;
    width: 12rem;
    height: 100%;
    overflow-y: auto;
    padding: calc(var(--toolbar-space) + 0.25rem) 0.5rem 3rem;
    border-right: 0.5px solid var(--separator);
    background: var(--glass);
    backdrop-filter: var(--blur);
  }

  header {
    padding: 0 0.5rem 0.5rem;
  }

  h2 {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  .empty {
    margin: 0;
    padding: 0 0.5rem;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .entries {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    position: relative;
    display: flex;
    align-items: center;
    border-top: 2px solid transparent;
  }

  li.drop-before {
    border-top-color: var(--accent);
  }

  li.end {
    height: 1.5rem;
  }

  .entry {
    flex: 1;
    min-width: 0;
    padding: 0.25rem 0.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .entry .text {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry:hover {
    background: var(--control-hover);
  }

  .entry.current {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .shift {
    display: flex;
    opacity: 0;
  }

  li:hover .shift,
  li:focus-within .shift {
    opacity: 1;
  }

  .shift button {
    width: 1.25rem;
    height: 1.25rem;
    padding: 0;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    color: var(--label-secondary);
    font: var(--headline);
  }

  .shift button:hover:not(:disabled) {
    background: var(--control-hover);
  }

  .shift button:disabled {
    color: var(--label-tertiary);
  }
</style>
