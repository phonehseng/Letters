<script lang="ts">
  // the small floating bar beside selected text: Highlight, Underline,
  // Strike Through, Note and Copy. it appears when a drag across text
  // ends and goes away with the selection. it is a toolbar: Tab reaches
  // it, and the arrow keys move between its buttons.
  import { nextIndex } from "../keyboard";
  import { tooltipFor, type ToolName } from "../tools";

  interface Props {
    /** where it goes, in css pixels of the scroll content */
    left: number;
    top: number;
    onHighlight: () => void;
    onUnderline: () => void;
    onStrike: () => void;
    onNote: () => void;
    onCopy: () => void;
  }

  let { left, top, onHighlight, onUnderline, onStrike, onNote, onCopy }: Props = $props();

  const items: { label: string; tool: ToolName; run: () => void }[] = $derived([
    { label: "Highlight", tool: "highlightText", run: onHighlight },
    { label: "Underline", tool: "underlineText", run: onUnderline },
    { label: "Strike Through", tool: "strikeText", run: onStrike },
    { label: "Note", tool: "addNote", run: onNote },
    { label: "Copy", tool: "copyText", run: onCopy },
  ]);
  let buttons: HTMLButtonElement[] = $state([]);

  function onKeyDown(event: KeyboardEvent) {
    const current = buttons.findIndex((button) => button === document.activeElement);
    const target = nextIndex(current, event.key, items.length, { orientation: "horizontal", wrap: true });
    if (target === null) return;
    event.preventDefault();
    buttons[target]?.focus();
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
  class="glass bar"
  role="toolbar"
  aria-label="Selected text"
  data-page-ui
  style:left="{left}px"
  style:top="{top}px"
  onkeydown={onKeyDown}
>
  {#each items as item, index (item.label)}
    <button
      class="item"
      title={tooltipFor(item.tool)}
      tabindex={index === 0 ? 0 : -1}
      bind:this={buttons[index]}
      onpointerdown={(event) => event.preventDefault()}
      onclick={item.run}
    >
      {item.label}
    </button>
  {/each}
</div>

<style>
  .bar {
    position: absolute;
    z-index: 2;
    display: flex;
    flex-wrap: wrap;
    gap: 0.125rem;
    max-width: calc(100% - 1rem);
    padding: var(--group-padding);
    animation: arrive 0.15s var(--ease);
  }

  .item {
    padding: 0.25rem 0.625rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    font: var(--body);
    font-weight: 500;
    white-space: nowrap;
  }

  .item:hover {
    background: var(--control-hover);
  }

  .item:active {
    background: var(--control-active);
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(0.25rem);
    }
  }
</style>
