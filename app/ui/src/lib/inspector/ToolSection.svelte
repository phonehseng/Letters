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
  import { tick, type Snippet } from "svelte";
  import Icon from "../Icon.svelte";
  import { tools, type ToolHelp, type ToolName } from "../tools";

  interface Props {
    tool: ToolName;
    open: boolean;
    onToggle: () => void;
    /** the one button; leave out for a section that only shows things (the comments list) */
    actionLabel?: string;
    /** the "what will happen" sentence */
    effect?: string;
    /** false while the settings are incomplete; the effect line says what is missing */
    ready?: boolean;
    /** the click that pressed the button: `detail` is 0 when it came from the keyboard */
    onAction?: (event: MouseEvent) => Promise<unknown> | void;
    /** the button shows as pressed while its page tool is in use */
    pressed?: boolean;
    children?: Snippet;
    /** a quieter second button, such as "Show Whole Page" */
    secondary?: Snippet;
  }

  // the effect sentence is called effectLine here, because `effect` would clash with svelte's $effect
  let { tool, open, onToggle, actionLabel, effect: effectLine = "", ready = true, onAction, pressed, children, secondary }: Props =
    $props();

  const help: ToolHelp = $derived(tools[tool]);
  let busy = $state(false);
  const bodyId = $derived(`tool-${tool}`);
  let element = $state<HTMLElement>();

  // an opened section scrolls itself into view, so one far down the list
  // (Comments, say) shows what it opened instead of opening out of sight
  $effect(() => {
    if (open) tick().then(() => element?.scrollIntoView({ block: "nearest" }));
  });

  async function run(event: MouseEvent) {
    if (busy || !ready) return;
    busy = true;
    try {
      await onAction?.(event);
    } finally {
      busy = false;
    }
  }
</script>

<section class="tool" class:open data-tool={tool} bind:this={element}>
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
      {#if actionLabel}
        <div class="actions">
          {@render secondary?.()}
          <button
            class="button primary action"
            disabled={!ready || busy}
            aria-pressed={pressed}
            aria-describedby={`${bodyId}-effect`}
            onclick={run}
          >
            {actionLabel}
          </button>
        </div>
      {/if}
      {#if effectLine}
        <p class="effect" id={`${bodyId}-effect`} aria-live="polite">{effectLine}</p>
      {/if}
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
