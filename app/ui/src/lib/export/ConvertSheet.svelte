<script lang="ts">
  // Convert File…: choose a file (or start with the one open), choose the
  // type it should become, then where the copy goes. the types are the
  // shell's list for that file (convert_file.rs); one that needs a program
  // this computer doesn't have stays in the list, greyed, saying so. pdfs
  // and pictures have tools of their own with more choices, so the sheet
  // sends them there instead of offering a poorer copy of those tools.
  import { onMount } from "svelte";
  import { asCommandError } from "../api";
  import { revealDocument } from "../organise/organiseApi";
  import { chooseConvertDestination, chooseFileToConvert, convertFile, convertFileChoices } from "./exportApi";
  import { elsewhereSentence, fileName, firstAvailable, savedSentence, suggestedPath, type ConvertChoices, type ConvertedFile, type ConvertFormat } from "./convertFile";

  interface Props {
    /** the file open in this window, if any */
    initialPath: string | null;
    onClose: () => void;
    /** a pdf: its Export To…, in this window or a new one */
    onExportPdf: (path: string) => void;
    /** pictures: New from Images… with this one in it */
    onPicturesToPdf: (path: string) => void;
    /** the converted copy, opened in Letters */
    onOpen: (path: string) => void;
  }

  let { initialPath, onClose, onExportPdf, onPicturesToPdf, onOpen }: Props = $props();

  let path = $state<string | null>(null);
  let choices = $state<ConvertChoices | null>(null);
  let format = $state<ConvertFormat | null>(null);
  let working = $state(false);
  let converted = $state<ConvertedFile | null>(null);
  let failure = $state<string | null>(null);
  let sheet = $state<HTMLDivElement>();

  const chosen = $derived(choices?.targets.find((target) => target.format === format) ?? null);
  const elsewhere = $derived(choices ? elsewhereSentence(choices.source) : null);

  async function useFile(next: string) {
    path = next;
    choices = null;
    converted = null;
    failure = null;
    try {
      choices = await convertFileChoices(next);
      format = firstAvailable(choices.targets);
    } catch (problem) {
      failure = asCommandError(problem).message;
    }
  }

  async function chooseFile() {
    const next = await chooseFileToConvert();
    if (next) await useFile(next);
  }

  async function convertNow() {
    if (!path || !chosen) return;
    const destination = await chooseConvertDestination(suggestedPath(path, chosen.extension), chosen.extension, chosen.label);
    if (!destination) return;
    working = true;
    failure = null;
    converted = null;
    try {
      converted = await convertFile(path, chosen.format, destination);
    } catch (problem) {
      failure = asCommandError(problem).message;
    } finally {
      working = false;
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
  }

  onMount(() => {
    if (initialPath) void useFile(initialPath);
    // the keyboard starts on the first thing to do: the file, or its types
    sheet?.querySelector<HTMLElement>("button.primary, input:checked")?.focus();
  });
</script>

<div class="backdrop" role="presentation" onkeydown={onKeyDown}>
  <div class="glass sheet" bind:this={sheet} role="dialog" aria-modal="true" aria-labelledby="convert-title">
    <h2 id="convert-title">Convert File</h2>

    <div class="file">
      {#if path}
        <span class="name" title={path}>{fileName(path)}</span>
        <button class="button" onclick={chooseFile} disabled={working}>Choose Another…</button>
      {:else}
        <span class="name quiet">Choose the file to convert. It stays as it is; the new type is a copy.</span>
        <button class="button primary" onclick={chooseFile}>Choose a File…</button>
      {/if}
    </div>

    {#if path && elsewhere}
      <p class="about-line">{elsewhere}</p>
    {:else if choices && choices.targets.length > 0}
      <div class="columns">
        <fieldset>
          <legend class="label">Convert to</legend>
          {#each choices.targets as target (target.format)}
            <label class="choice" class:later={!target.available}>
              <input type="radio" name="convert-format" value={target.format} bind:group={format} disabled={!target.available || working} />
              <span>{target.label}{#if !target.available}<span class="reason">{target.note}</span>{/if}</span>
            </label>
          {/each}
        </fieldset>
        {#if chosen}
          <p class="about">{chosen.note}</p>
        {/if}
      </div>
      <p class="about-line">The file is read as it was last saved, and it doesn’t change.</p>
    {/if}

    {#if converted}
      <div class="result" role="status">
        <p>{savedSentence(converted)}</p>
        {#each converted.notes as note (note)}<p class="note">{note}</p>{/each}
        <div class="links">
          <button class="button" onclick={() => converted && revealDocument(converted.path).catch(() => {})}>Show in Folder</button>
          <button class="button" onclick={() => converted && onOpen(converted.path)}>Open</button>
        </div>
      </div>
    {/if}
    {#if failure}<p class="failure" role="alert">{failure}</p>{/if}

    <div class="buttons">
      <button class="button" onclick={onClose}>{converted ? "Done" : "Cancel"}</button>
      {#if path && choices?.source === "pdf"}
        <button class="button primary" onclick={() => path && onExportPdf(path)}>Export To…</button>
      {:else if path && choices?.source === "picture"}
        <button class="button primary" onclick={() => path && onPicturesToPdf(path)}>New from Images…</button>
      {:else if path && !elsewhere}
        <button class="button primary" onclick={convertNow} disabled={working || !chosen}>{working ? "Converting…" : "Convert…"}</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: var(--toolbar-space);
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    width: min(34rem, calc(100% - 2rem));
    padding: 1rem 1.25rem;
  }

  h2 {
    margin: 0 0 0.75rem;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .file {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-bottom: 0.75rem;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--headline);
  }

  .name.quiet {
    white-space: normal;
    font: var(--body);
    color: var(--label-secondary);
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  fieldset {
    flex: 1 1 14rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .label {
    padding: 0;
    margin-bottom: 0.25rem;
    font: var(--subheadline);
    color: var(--label-secondary);
  }

  .choice {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.25rem 0;
    font: var(--body);
  }

  .later {
    color: var(--label-tertiary);
  }

  .reason {
    display: block;
    font: var(--subheadline);
    color: var(--label-secondary);
  }

  .about {
    flex: 1 1 10rem;
    margin: 1.5rem 0 0;
    font: var(--body);
    color: var(--label-secondary);
  }

  .about-line,
  .result p,
  .failure {
    margin: 0.5rem 0 0;
    font: var(--body);
  }

  .about-line,
  .note {
    color: var(--label-secondary);
  }

  .failure {
    color: var(--danger);
  }

  .links {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1rem;
  }
</style>
