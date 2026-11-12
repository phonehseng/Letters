<script lang="ts">
  // Export To…: one sheet listing what this computer can make from the
  // pdf, with each format's few settings, the pages, then the save panel,
  // progress with Cancel, and a result line in plain words. the logic is
  // in exportChoices.ts; this component only draws it.
  import { onMount } from "svelte";
  import type { DocumentState } from "../api";
  import Choices from "../inspector/Choices.svelte";
  import * as api from "./exportApi";
  import {
    checkOptions,
    closenessRows,
    defaultOptions,
    durationHint,
    effectiveStyle,
    formatChoices,
    progressText,
    saveFilter,
    suggestedName,
    type ExportOptions,
    type ExportOutcome,
    type FormatChoice,
    type Style,
  } from "./exportChoices";

  interface Props {
    document: DocumentState;
    /** distance from the top of the window, below the toolbar */
    top: number;
    /** the Word style Settings chose to start with */
    wordStyle?: Style;
    onClose: () => void;
  }

  let { document: doc, top, wordStyle = "exactMatch", onClose }: Props = $props();

  let choices = $state<FormatChoice[]>([]);
  let options = $state<ExportOptions>(defaultOptions());
  let hasWord = $state(false);
  let problem = $state<string | null>(null);
  let running = $state<{ id: number; text: string; fraction: number } | null>(null);
  let outcome = $state<ExportOutcome | null>(null);
  let sheet = $state<HTMLDivElement>();

  const current = $derived(choices.find((choice) => choice.id === options.format));
  const checked = $derived(checkOptions(options, doc.pages.length, choices));
  const hint = $derived(checked.ok && choices.length > 0 ? durationHint(checked.request, hasWord) : null);

  onMount(() => {
    options = defaultOptions(wordStyle);
    api
      .exportCapabilities()
      .then((found) => {
        choices = formatChoices(found);
        hasWord = !!found.wordInstalled;
      })
      .catch((error) => (problem = String(error?.message ?? error)));
    const stop = api.listenForExportProgress((event) => {
      if (running && event.exportId === running.id) {
        running = { ...running, fraction: event.fraction, text: progressText(event.fraction, event.page) };
      }
    });
    sheet?.querySelector<HTMLElement>("input:checked")?.focus();
    return stop;
  });

  function styleOf(format: "word" | "webPage" | "text"): Style {
    return effectiveStyle(
      choices.find((choice) => choice.id === format),
      options.styles[format],
    );
  }

  async function start() {
    if (!checked.ok) {
      problem = checked.error;
      return;
    }
    problem = null;
    const destination = await api.chooseExportLocation(suggestedName(doc.path, options), saveFilter(options));
    if (!destination) return;
    const id = Date.now();
    running = { id, text: progressText(0, null), fraction: 0 };
    try {
      outcome = await api.exportDocument(doc.id, id, { request: checked.request, pages: checked.pages, destination });
    } catch (error) {
      const message = (error as { message?: string })?.message ?? String(error);
      problem = message;
    }
    running = null;
  }

  function cancel() {
    if (running) api.cancelExport(running.id);
    else onClose();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.stopPropagation();
    cancel();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="glass sheet" role="dialog" aria-modal="true" aria-label="Export" tabindex="-1" style:top="{top + 8}px" bind:this={sheet} onkeydown={onKeyDown}>
  <h2>Export “{doc.name}”</h2>

  {#if outcome}
    <section class="result" aria-live="polite">
      <p class="summary">{outcome.summary}</p>
      {#if outcome.verification}<p>{outcome.verification}</p>{/if}
      {#if outcome.closeness}
        <details>
          <summary>How close is it?</summary>
          <dl>
            {#each closenessRows(outcome.closeness) as row (row.label)}
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            {/each}
          </dl>
        </details>
      {/if}
      {#each [...outcome.standIns, ...outcome.notes] as note, index (index)}
        <p class="hint">{note}</p>
      {/each}
      <p class="hint files">Saved {outcome.files.length === 1 ? outcome.files[0] : `${outcome.files.length} files in ${outcome.files[0].replace(/[\\/][^\\/]*$/, "")}`}.</p>
    </section>
    <div class="buttons">
      <span class="spacer"></span>
      <button class="button" onclick={() => (outcome = null)}>Export Again…</button>
      <button class="button primary" onclick={onClose}>Done</button>
    </div>
  {:else}
    <div class="body">
      <fieldset class="formats" disabled={!!running}>
        <legend class="caption">Format</legend>
        {#each choices as choice (choice.id)}
          <label class="format" class:unavailable={!choice.available} title={choice.available ? choice.note : choice.note}>
            <input type="radio" name="export-format" value={choice.id} disabled={!choice.available} bind:group={options.format} />
            <span class="name">{choice.label}</span>
            {#if !choice.available}<span class="why">{choice.note}</span>{/if}
          </label>
        {/each}
      </fieldset>

      <div class="settings">
        {#if current?.styles}
          <fieldset class="styles" disabled={!!running}>
            <legend class="caption">Kind</legend>
            {#each current.styles as option (option.style)}
              {@const format = current.id as "word" | "webPage" | "text"}
              <label class="style" class:chosen={styleOf(format) === option.style} class:unavailable={!option.available}>
                <input type="radio" name="export-style" value={option.style} disabled={!option.available} bind:group={options.styles[format]} />
                <span class="name">{option.label}</span>
                <span class="explanation">{option.explanation}</span>
              </label>
            {/each}
          </fieldset>
        {:else if current}
          <p class="hint">{current.note}</p>
        {/if}

        {#if options.format === "images"}
          <Choices label="Picture" options={[{ value: "png", label: "PNG" }, { value: "jpeg", label: "JPEG" }, { value: "tiff", label: "TIFF" }]} bind:value={options.image} />
          <label class="setting">
            <span class="caption">Resolution (dpi)</span>
            <input class="field" type="number" min="10" max="2400" step="10" bind:value={options.dpi} />
          </label>
          <Choices label="Colour" options={[{ value: "color", label: "Colour" }, { value: "grayscale", label: "Gray" }]} bind:value={options.color} />
          {#if options.image === "jpeg"}
            <label class="setting">
              <span class="caption">Quality: {options.quality}</span>
              <input type="range" min="1" max="100" bind:value={options.quality} />
            </label>
          {:else if options.image === "tiff"}
            <label class="check"><input type="checkbox" bind:checked={options.multiPage} /> All pages in one file</label>
          {/if}
        {:else if options.format === "webPage"}
          <label class="check"><input type="checkbox" bind:checked={options.singleFile} /> One file, with pictures inside</label>
        {:else if options.format === "excel"}
          <Choices label="Sheets" options={[{ value: "sheetPerPage", label: "One per Page" }, { value: "sheetPerTable", label: "One per Table" }]} bind:value={options.excelLayout} />
        {/if}

        <fieldset class="setting" disabled={!!running}>
          <legend class="caption">Pages</legend>
          <div class="pages">
            <label class="check"><input type="radio" name="export-pages" value="all" bind:group={options.pages} /> All {doc.pages.length}</label>
            <label class="check"><input type="radio" name="export-pages" value="range" bind:group={options.pages} /> Pages</label>
            <input
              class="field range"
              aria-label="Pages to export"
              placeholder="such as 1-3, 7"
              bind:value={options.pagesText}
              onfocus={() => (options.pages = "range")}
            />
          </div>
        </fieldset>

        {#if hint}<p class="hint">{hint}</p>{/if}
      </div>
    </div>

    {#if running}
      <div class="progress" role="status" aria-live="polite">
        <progress max="1" value={running.fraction}></progress>
        <span>{running.text}</span>
      </div>
    {/if}
    {#if problem}<p class="problem" role="alert">{problem}</p>{/if}

    <div class="buttons">
      <span class="spacer"></span>
      <button class="button" onclick={cancel}>Cancel</button>
      <button class="button primary" disabled={!!running || choices.length === 0} onclick={start}>Export…</button>
    </div>
  {/if}
</div>

<style>
  .sheet {
    position: absolute;
    left: 50%;
    z-index: 5;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: min(40rem, calc(100% - 2rem));
    max-height: calc(100% - 5rem);
    overflow-y: auto;
    padding: 0.875rem 1rem;
    transform: translateX(-50%);
    box-shadow: var(--shadow-sheet);
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  .body {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }

  .formats {
    display: flex;
    flex: 0 1 13rem;
    flex-direction: column;
    gap: 0.125rem;
  }

  .format {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: baseline;
    gap: 0 0.5rem;
    padding: 0.25rem 0.375rem;
    border-radius: var(--radius-control);
  }

  .format:hover:not(.unavailable) {
    background: var(--control-hover);
  }

  .unavailable {
    color: var(--label-tertiary);
  }

  .why {
    grid-column: 2;
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
  }

  .settings {
    display: flex;
    flex: 1 1 18rem;
    flex-direction: column;
    gap: 0.625rem;
    min-width: 0;
  }

  /* the two styles side by side, each with the engine's sentence */
  .styles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }

  .styles legend {
    margin-bottom: 0.25rem;
  }

  .style {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.25rem 0.375rem;
    padding: 0.5rem;
    border-radius: var(--radius-group);
    background: var(--control);
  }

  .style.chosen {
    box-shadow: 0 0 0 1.5px var(--accent);
  }

  .style .name {
    font-weight: 600;
  }

  .explanation {
    grid-column: 1 / -1;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .pages {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .range {
    width: 9rem;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.375rem;
  }

  .progress {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  progress {
    flex: 1;
    accent-color: var(--accent);
  }

  .problem {
    margin: 0;
    color: var(--danger);
  }

  .result p {
    margin: 0 0 0.375rem;
  }

  .summary {
    font-weight: 600;
  }

  dl {
    display: grid;
    grid-template-columns: auto auto;
    gap: 0.125rem 1rem;
    margin: 0.5rem 0;
  }

  dt {
    color: var(--label-secondary);
  }

  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }

  .files {
    overflow-wrap: anywhere;
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
  }

  .spacer {
    flex: 1;
  }

  @media (max-width: 34rem) {
    .styles {
      grid-template-columns: 1fr;
    }
  }
</style>
