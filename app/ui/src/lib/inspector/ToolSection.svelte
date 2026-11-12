<script lang="ts">
  // one tool in the inspector, as a disclosure section.
  //
  // closed, it shows the tool's name and one line about it. open, it shows
  // what the word means when that may be new, the tool's few settings, one
  // clearly named button, and under the button a live sentence saying
  // exactly what will happen to which pages and to the file.
  //
  // only one section is open at a time (the tab decides), so there is
  // only ever one prominent button in the inspector.
  import type { Snippet } from "svelte";
  import Icon from "../Icon.svelte";
  import { tools, type ToolHelp, type ToolName } from "../tools";

  interface Props {
    tool: ToolName;
    open: boolean;
    onToggle: () => void;
    actionLabel: string;
    /** the "what will happen" sentence */
    effect: string;
    /** false while the settings are incomplete; the effect line says what is missing */
    ready?: boolean;
    onAction: () => Promise<unknown> | void;
    children?: Snippet;
    /** a quieter second button, such as "Show Whole Page" */
    secondary?: Snippet;
  }

  let { tool, open, onToggle, actionLabel, effect, ready = true, onAction, children, secondary }: Props = $props();

  const help: ToolHelp = $derived(tools[tool]);
  let busy = $state(false);
  const bodyId = $derived(`tool-${tool}`);

  async function run() {
    if (busy || !ready) return;
    busy = true;
    try {
      await onAction();
    } finally {
      busy = false;
    }
  }
</script>

<section class="tool" class:open data-tool={tool}>
  <h3>
    <button class="header" aria-expanded={open} aria-controls={bodyId} title={help.tooltip} onclick={onToggle}>
      <span class="chevron"><Icon name={open ? "chevronDown" : "chevronRight"} size={0.75} /></span>
      <span class="titles">
        <span class="name">{help.label}</span>
        {#if !open}
          <span class="summary">{help.tooltip}</span>
        {/if}
      </span>
    </button>
  </h3>
  {#if open}
    <div class="body" id={bodyId}>
      {#if help.meaning}
        <p class="meaning">{help.meaning}</p>
      {/if}
      {@render children?.()}
      <div class="actions">
        {@render secondary?.()}
        <button class="button primary action" disabled={!ready || busy} aria-describedby={`${bodyId}-effect`} onclick={run}>
          {actionLabel}
        </button>
      </div>
      <p class="effect" id={`${bodyId}-effect`} aria-live="polite">{effect}</p>
    </div>
  {/if}
</section>

<style>
  .tool {
    border-top: 0.5px solid var(--separator);
  }

  h3 {
    margin: 0;
    font: inherit;
  }

  .header {
    display: flex;
    align-items: flex-start;
    gap: 0.375rem;
    width: 100%;
    padding: 0.625rem 0.25rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    text-align: left;
  }

  .header:hover {
    background: var(--control);
  }

  .chevron {
    display: grid;
    flex-shrink: 0;
    margin-top: 0.125rem;
    color: var(--label-secondary);
  }

  .titles {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
  }

  .name {
    font: var(--headline);
  }

  .summary {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    padding: 0 0.25rem 0.875rem 1.375rem;
  }

  .meaning {
    margin: 0;
    color: var(--label-secondary);
    line-height: 1.125rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .action:disabled {
    opacity: 0.45;
  }

  .effect {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    line-height: 0.9375rem;
    letter-spacing: var(--subheadline-spacing);
  }
</style>
