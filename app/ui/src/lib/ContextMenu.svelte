<script lang="ts" module>
  export interface ContextItem {
    label: string;
    title?: string;
    disabled?: boolean;
    /** a line before this item */
    separatorBefore?: boolean;
    run: () => void;
  }
</script>

<script lang="ts">
  // a small menu at the pointer, for right-clicking thumbnails. it
  // behaves like a desktop menu: arrows move, Enter chooses, Escape or a
  // click elsewhere closes it and gives the focus back.
  import { onMount, tick } from "svelte";
  import { nextIndex } from "./keyboard";

  interface Props {
    /** where the menu opens, in css pixels from the window's top left */
    x: number;
    y: number;
    label: string;
    items: ContextItem[];
    onClose: () => void;
  }

  let { x, y, label, items, onClose }: Props = $props();

  let menu = $state<HTMLDivElement>();
  let buttons: HTMLButtonElement[] = $state([]);
  const opener = document.activeElement as HTMLElement | null;
  // kept inside the window, so a click near an edge still shows every item
  let place = $state({ left: 0, top: 0 });

  function close() {
    onClose();
    opener?.focus();
  }

  function choose(item: ContextItem) {
    if (item.disabled) return;
    close();
    item.run();
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" || event.key === "Tab") {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const target = nextIndex(current, event.key, items.length, { orientation: "vertical", wrap: true });
    if (target === null) return;
    event.preventDefault();
    buttons[target]?.focus();
  }

  onMount(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!menu?.contains(event.target as Node)) close();
    };
    window.addEventListener("pointerdown", onPointerDown, true);
    tick().then(() => {
      const width = menu?.offsetWidth ?? 0;
      const height = menu?.offsetHeight ?? 0;
      place = {
        left: Math.max(4, Math.min(x, window.innerWidth - width - 4)),
        top: Math.max(4, Math.min(y, window.innerHeight - height - 4)),
      };
      buttons.find((button) => !button.disabled)?.focus();
    });
    return () => window.removeEventListener("pointerdown", onPointerDown, true);
  });
</script>

<div
  class="glass menu"
  role="menu"
  aria-label={label}
  tabindex="-1"
  style:left="{place.left}px"
  style:top="{place.top}px"
  bind:this={menu}
  onkeydown={onKeyDown}
>
  {#each items as item, index (item.label)}
    {#if item.separatorBefore}
      <div class="separator" role="separator"></div>
    {/if}
    <button
      class="item"
      role="menuitem"
      tabindex="-1"
      aria-disabled={item.disabled}
      title={item.title}
      bind:this={buttons[index]}
      onclick={() => choose(item)}
    >
      {item.label}
    </button>
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 12rem;
    padding: var(--group-padding);
    outline: none;
  }

  .item {
    height: 1.75rem;
    padding: 0 0.625rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .item:hover:not([aria-disabled="true"]),
  .item:focus-visible {
    background: var(--control-hover);
  }

  .item[aria-disabled="true"] {
    color: var(--label-tertiary);
  }

  .separator {
    height: 0.5px;
    margin: 0.25rem 0.625rem;
    background: var(--separator);
  }
</style>
