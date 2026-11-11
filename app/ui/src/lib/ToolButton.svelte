<script lang="ts">
  // a toolbar button: a symbol with its name underneath.
  //
  // the label and tooltip come from tools.ts, so no button can be added
  // without a plain-english description. the name stays visible because
  // a symbol alone makes a new student guess (see docs/DESIGN.md).
  import Icon, { type IconName } from "./Icon.svelte";
  import { tools, tooltipFor, type ToolName } from "./tools";

  interface Props {
    name: ToolName;
    icon: IconName;
    onClick: () => void;
    /** for on/off tools such as night mode */
    pressed?: boolean;
    /** show only the symbol. for the few everyone knows, like back */
    symbolOnly?: boolean;
    /** hide the name on a narrow window; screen readers still hear it */
    hideLabel?: boolean;
    /** for a pull-down button: whether its menu is showing */
    menuOpen?: boolean;
    /** the button element, so a menu can give focus back to it */
    element?: HTMLButtonElement;
    onKeyDown?: (event: KeyboardEvent) => void;
  }

  let {
    name,
    icon,
    onClick,
    pressed,
    symbolOnly = false,
    hideLabel = false,
    menuOpen,
    element = $bindable(),
    onKeyDown,
  }: Props = $props();
  const tool = $derived(tools[name]);
  const isMenuButton = $derived(menuOpen !== undefined);
</script>

<button
  class="tool"
  class:pressed
  class:open={menuOpen}
  class:symbol-only={symbolOnly || hideLabel}
  title={tooltipFor(name)}
  aria-label={symbolOnly ? tool.label : undefined}
  aria-pressed={pressed}
  aria-haspopup={isMenuButton ? "menu" : undefined}
  aria-expanded={isMenuButton ? menuOpen : undefined}
  bind:this={element}
  onclick={onClick}
  onkeydown={onKeyDown}
>
  <Icon name={icon} />
  {#if !symbolOnly}
    <span class:visually-hidden={hideLabel}>{tool.label}</span>
  {/if}
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
    /* concentric with the glass group this button sits in */
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--label);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
    white-space: nowrap;
    transition: background 0.15s var(--ease);
  }

  .tool.symbol-only {
    min-width: 2.75rem;
  }

  .tool span {
    color: var(--label-secondary);
  }

  .tool:hover {
    background: var(--control-hover);
  }

  /* every button needs a pressed look, or it feels like it ignored the click */
  .tool:active,
  .tool.open {
    background: var(--control-active);
  }

  /* an on/off tool that is on: the one place the toolbar uses the accent */
  .tool.pressed {
    background: var(--accent-soft);
    color: var(--accent-label);
  }

  .tool.pressed span {
    color: var(--accent-label);
  }
</style>
