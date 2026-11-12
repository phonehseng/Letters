<script lang="ts">
  // joins whole pdfs, in the order of a list the person arranges, into a
  // new file, then offers to open it. the list starts with this document.
  import { untrack } from "svelte";
  import { asCommandError, chooseSaveLocation, newWindow } from "../../api";
  import Icon from "../../Icon.svelte";
  import type { ToolContext } from "../../inspector";
  import { chooseFiles, fileNameOf, mergeDocuments, pathBeside, pdfPageCount, stemOf } from "../../pageApi";
  import { mergeEffect } from "../../pageEffects";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  interface Entry {
    path: string;
    name: string;
    pages: number | null;
    problem: string | null;
  }

  let entries = $state<Entry[]>([]);
  let resultName = $state("");
  $effect(() => {
    const path = context.document.path;
    untrack(() => {
      if (entries.length === 0) addPaths([path]);
      if (resultName === "") resultName = `${stemOf(path)} merged`;
    });
  });

  const ready = $derived(entries.length >= 2 && entries.every((entry) => !entry.problem) && resultName.trim() !== "");
  const outcome = $derived(entries.find((entry) => entry.problem)?.problem ?? mergeEffect(entries.length));

  async function addPaths(paths: string[]) {
    // each file is listed once; the same file twice is almost always a slip
    const fresh = paths.filter((path) => !entries.some((entry) => entry.path === path));
    const added = fresh.map((path): Entry => ({ path, name: fileNameOf(path), pages: null, problem: null }));
    entries = [...entries, ...added];
    for (const entry of added) {
      try {
        const pages = await pdfPageCount(entry.path);
        entries = entries.map((other) => (other.path === entry.path ? { ...other, pages } : other));
      } catch (problem) {
        const message = `“${entry.name}”: ${asCommandError(problem).message}`;
        entries = entries.map((other) => (other.path === entry.path ? { ...other, problem: message } : other));
      }
    }
  }

  async function addFiles() {
    addPaths(await chooseFiles());
  }

  function move(index: number, step: -1 | 1) {
    const target = index + step;
    if (target < 0 || target >= entries.length) return;
    const next = [...entries];
    [next[index], next[target]] = [next[target], next[index]];
    entries = next;
  }

  function remove(index: number) {
    entries = entries.filter((_, other) => other !== index);
  }

  async function merge() {
    const suggested = pathBeside(context.document.path, `${resultName.trim().replace(/\.pdf$/i, "")}.pdf`);
    const savePath = await chooseSaveLocation(suggested);
    if (!savePath) return;
    try {
      const written = await mergeDocuments(
        entries.map((entry) => entry.path),
        entries.map((entry) => stemOf(entry.path)),
        savePath,
      );
      context.notify(`Merged ${entries.length} PDFs into “${fileNameOf(written)}”.`, {
        label: "Open",
        run: () => newWindow(written),
      });
    } catch (problem) {
      context.showError(asCommandError(problem).message);
    }
  }
</script>

<ToolSection tool="mergeDocuments" {open} {onToggle} actionLabel="Merge…" effect={outcome} {ready} onAction={merge}>
  <div class="setting">
    <span class="caption" id="merge-list-caption">PDFs, in Order</span>
    <ol class="files" aria-labelledby="merge-list-caption">
      {#each entries as entry, index (entry.path)}
        <li class:problem={entry.problem}>
          <span class="file">
            <span class="name">{entry.path === context.document.path ? `${entry.name} (this document)` : entry.name}</span>
            <span class="pages">{entry.pages === null ? "" : `${entry.pages} ${entry.pages === 1 ? "page" : "pages"}`}</span>
          </span>
          <button class="small" title="Move Up" aria-label={`Move ${entry.name} up`} disabled={index === 0} onclick={() => move(index, -1)}>
            <Icon name="arrowUp" size={0.875} />
          </button>
          <button
            class="small"
            title="Move Down"
            aria-label={`Move ${entry.name} down`}
            disabled={index === entries.length - 1}
            onclick={() => move(index, 1)}
          >
            <Icon name="arrowDown" size={0.875} />
          </button>
          <button class="small" title="Remove from the list" aria-label={`Remove ${entry.name} from the list`} onclick={() => remove(index)}>
            <Icon name="close" size={0.625} />
          </button>
        </li>
      {/each}
    </ol>
    <button class="button" onclick={addFiles}>Add PDFs…</button>
    {#if context.document.dirty && entries.some((entry) => entry.path === context.document.path)}
      <p class="hint">This document is merged as it was last saved. Save first to include your changes.</p>
    {/if}
  </div>
  <label class="setting">
    <span class="caption">Name of the New PDF</span>
    <input class="field" type="text" bind:value={resultName} />
  </label>
</ToolSection>

<style>
  .files {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 0.125rem;
    padding: 0.25rem 0.25rem 0.25rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  li.problem {
    box-shadow: inset 0 0 0 1px var(--danger);
  }

  .file {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pages {
    color: var(--label-secondary);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
  }

  .small {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 1.5rem;
    height: 1.5rem;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
  }

  .small:hover:not(:disabled) {
    background: var(--control-hover);
  }

  .small:disabled {
    color: var(--label-tertiary);
  }
</style>
