<script lang="ts">
  // "Export as Code File…": formats on the leading side as radio buttons,
  // what the chosen one is for beside them, then the save panel. formats
  // no engine writes yet stay in the list, greyed, saying so.
  import { onMount } from "svelte";
  import { chooseCodeSaveLocation, exportAsCode } from "./codeApi";
  import { CODE_EXPORT_CHOICES, CODE_EXPORT_LATER, choiceFor, exportLine, suggestedExportPath, type CodeExportFormat } from "./codeExport";
  import { errorText } from "./codeSession.svelte";

  interface Props {
    source: { path: string; kind: "writer" | "pdf"; edited: boolean };
    onClose: () => void;
  }

  let { source, onClose }: Props = $props();
  let format = $state<CodeExportFormat>("markdown");
  let working = $state(false);
  let result = $state<string | null>(null);
  let failure = $state<string | null>(null);
  let sheet = $state<HTMLDivElement>();
  const chosen = $derived(choiceFor(format));

  async function exportNow() {
    const destination = await chooseCodeSaveLocation(suggestedExportPath(source.path, format), chosen.extension, chosen.label);
    if (!destination) return;
    working = true;
    failure = null;
    try {
      result = (await exportAsCode(source.path, source.kind, format, destination)).sentence;
    } catch (problem) {
      failure = errorText(problem);
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

  // the keyboard starts on the format list, the first thing to choose
  onMount(() => sheet?.querySelector<HTMLInputElement>("input:checked")?.focus());
</script>

<div class="backdrop" role="presentation" onkeydown={onKeyDown}>
  <div class="glass sheet" bind:this={sheet} role="dialog" aria-modal="true" aria-labelledby="export-code-title">
    <h2 id="export-code-title">Export as Code File</h2>
    <div class="columns">
      <fieldset>
        <legend class="visually-hidden">Format</legend>
        {#each CODE_EXPORT_CHOICES as choice (choice.format)}
          <label class="choice">
            <input type="radio" name="code-format" value={choice.format} bind:group={format} />
            <span>{choice.label} <span class="extension">.{choice.extension}</span></span>
          </label>
        {/each}
        {#each CODE_EXPORT_LATER as later (later.label)}
          <label class="choice later">
            <input type="radio" name="code-format" disabled />
            <span>{later.label}<span class="reason">{later.reason}</span></span>
          </label>
        {/each}
      </fieldset>
      <div class="about">
        <h3>{chosen.label}</h3>
        <p>{chosen.summary}</p>
      </div>
    </div>
    {#if result}<p class="result" role="status">{result}</p>{/if}
    {#if failure}<p class="failure" role="alert">{failure}</p>{/if}
    <div class="buttons">
      <button class="button" onclick={onClose}>{result ? "Done" : "Cancel"}</button>
      <button class="button primary" onclick={exportNow} disabled={working}>{working ? "Exporting…" : "Export…"}</button>
    </div>
    <p class="line">{exportLine(format, source.kind, source.edited)}</p>
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: var(--toolbar-space);
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    width: min(36rem, calc(100% - 2rem));
    padding: 1rem 1.25rem;
  }

  h2 {
    margin: 0 0 0.75rem;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  fieldset {
    flex: 1 1 12rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .choice {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.25rem 0;
    font: var(--body);
  }

  .extension,
  .reason {
    color: var(--label-secondary);
  }

  .reason {
    display: block;
    font: var(--subheadline);
  }

  .later {
    color: var(--label-tertiary);
  }

  .about {
    flex: 1 1 12rem;
  }

  h3 {
    margin: 0.25rem 0;
    font: var(--headline);
  }

  .about p,
  .line,
  .result,
  .failure {
    font: var(--body);
  }

  .failure {
    color: var(--danger);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.75rem;
  }

  .line {
    margin: 0.5rem 0 0;
    color: var(--label-secondary);
    text-align: right;
  }
</style>
