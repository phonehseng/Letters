<script lang="ts">
  // a toolbar or inspector button of the writer: a symbol with its name,
  // words from tools.ts, the same look as the viewer's ToolButton.
  import { tools, tooltipFor, type ToolName } from "../tools";
  import WriterIcon, { type WriterIconName } from "./WriterIcon.svelte";

  interface Props {
    name: ToolName;
    icon: WriterIconName;
    onClick: () => void;
    pressed?: boolean;
    hideLabel?: boolean;
    disabled?: boolean;
    /** small square buttons in the inspector (B, I, U) show only the symbol */
    compact?: boolean;
  }

  let { name, icon, onClick, pressed, hideLabel = false, disabled = false, compact = false }: Props = $props();
  const tool = $derived(tools[name]);
</script>

<button
  class="tool"
  class:pressed
  class:compact
  class:symbol-only={hideLabel}
  title={tooltipFor(name)}
  aria-label={compact || hideLabel ? tool.label : undefined}
  aria-pressed={pressed}
  {disabled}
  onclick={onClick}
>
  <WriterIcon name={icon} size={compact ? 1.125 : 1.25} />
  {#if !compact}<span class:visually-hidden={hideLabel}>{tool.label}</span>{/if}
</button>

<style>
  .tool {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.125rem;
    min-width: 3.5rem;
    height: 2.75rem;
    padding: 0 0.5rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
    white-space: nowrap;
    transition: background 0.15s var(--ease);
  }

  .tool.symbol-only {
    min-width: 2.75rem;
  }

  .tool.compact {
    min-width: 0;
    width: 2rem;
    height: 1.75rem;
    padding: 0;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .tool span {
    color: var(--label-secondary);
  }

  .tool:hover:not(:disabled) {
    background: var(--control-hover);
  }

  .tool:active:not(:disabled) {
    background: var(--control-active);
  }

  .tool:disabled {
    color: var(--label-tertiary);
  }

  .tool.pressed {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .tool.pressed span {
    color: var(--accent-label);
  }
</style>
