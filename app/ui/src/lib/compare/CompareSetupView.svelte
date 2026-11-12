<script lang="ts">
  // before comparing: the older and the newer version side by side, each
  // with Choose…, a Swap between them, the options, and Compare. two pdfs
  // dropped anywhere on the screen fill both.
  import { tooltipFor } from "../tools";
  import { choosePdf, type CompareOptions } from "./compareApi";
  import CompareOptionsForm from "./CompareOptionsForm.svelte";
  import { chosenFile, isReady, setupEffect, swap, type CompareSetup } from "./setup";

  interface Props {
    setup: CompareSetup;
    options: CompareOptions;
    onSetup: (next: CompareSetup) => void;
    onOptions: (next: CompareOptions) => void;
    onStart: () => void;
  }

  let { setup, options, onSetup, onOptions, onStart }: Props = $props();

  async function choose(which: "older" | "newer") {
    const path = await choosePdf();
    if (path) onSetup({ ...setup, [which]: chosenFile(path) });
  }
</script>

<div class="setup">
  <h1>Compare</h1>
  <div class="versions">
    <div class="version">
      <h2 id="older-title">Older version</h2>
      <p class="name" aria-labelledby="older-title">{setup.older?.name ?? "None chosen"}</p>
      <button class="button" title={tooltipFor("compareChooseOlder")} onclick={() => choose("older")}>Choose Older…</button>
    </div>
    <button class="button swap" title={tooltipFor("compareSwap")} disabled={!setup.older && !setup.newer} onclick={() => onSetup(swap(setup))}>
      Swap
    </button>
    <div class="version">
      <h2 id="newer-title">Newer version</h2>
      <p class="name" aria-labelledby="newer-title">{setup.newer?.name ?? "None chosen"}</p>
      <button class="button" title={tooltipFor("compareChooseNewer")} onclick={() => choose("newer")}>Choose Newer…</button>
    </div>
  </div>

  <CompareOptionsForm {options} onChange={onOptions} />

  <div class="actions">
    <button class="button primary" title={tooltipFor("compareStart")} disabled={!isReady(setup)} onclick={onStart} aria-describedby="compare-setup-effect">Compare</button>
  </div>
  <p class="hint" id="compare-setup-effect">{setupEffect(setup)}</p>
</div>

<style>
  .setup {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    width: min(40rem, calc(100% - 2rem));
    margin: 0 auto;
    padding: calc(var(--toolbar-space) + 1.25rem) 0 3rem;
  }

  h1 {
    margin: 0;
    font: var(--large-title);
    letter-spacing: var(--large-title-spacing);
  }

  .versions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 1rem;
  }

  .version {
    display: flex;
    flex: 1 1 14rem;
    flex-direction: column;
    gap: 0.5rem;
    min-width: 0;
    padding: 1rem;
    border-radius: var(--radius-group);
    background: var(--control);
  }

  h2 {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  .name {
    margin: 0;
    overflow-wrap: anywhere;
    font: var(--headline);
  }

  .version .button {
    align-self: flex-start;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
