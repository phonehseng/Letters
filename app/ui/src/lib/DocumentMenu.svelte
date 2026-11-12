<script lang="ts">
  // the document menu: a pull-down on the document's name, where the HIG
  // puts commands about the document itself (Save, Save As, Undo...).
  // while there are unsaved changes the name is followed by "— Edited",
  // as in the title bar of a mac app.
  //
  // like the View menu it behaves like a desktop menu: arrows move, Enter
  // or Space choose, Escape closes and gives focus back to the button.
  // disabled items stay in the list (greyed, with a tooltip saying why),
  // so the menu never changes shape under the pointer.
  import { tick } from "svelte";
  import type { Command } from "./commands";
  import { documentMenuEntries, SEPARATOR, type DocumentMenuItem } from "./documentMenu";
  import Icon from "./Icon.svelte";
  import { nextIndex } from "./keyboard";
  import { tools, tooltipFor } from "./tools";
  import type { DocumentState } from "./api";

  interface Props {
    document: DocumentState;
    onCommand: (command: Command) => void;
  }

  let { document: doc, onCommand }: Props = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement>();
  let anchor = $state<HTMLDivElement>();
  let itemElements: HTMLButtonElement[] = $state([]);

  const entries = $derived(documentMenuEntries(doc));
  const items = $derived(entries.filter((entry): entry is DocumentMenuItem => entry !== SEPARATOR));

  async function show() {
    open = true;
    await tick();
    itemElements[0]?.focus();
  }

  function hide(giveFocusBack: boolean) {
    open = false;
    if (giveFocusBack) button?.focus();
  }

  function choose(item: DocumentMenuItem) {
    // a disabled item does nothing, like a greyed menu item on any desktop
    if (!item.enabled) return;
    hide(true);
    onCommand(item.command);
  }

  function onButtonKeyDown(event: KeyboardEvent) {
    if (event.key !== "ArrowDown" || open) return;
    event.preventDefault();
    show();
  }

  function onMenuKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.stopPropagation();
      hide(true);
      return;
    }
    if (event.key === "Tab") {
      hide(false);
      return;
    }
    const current = itemElements.indexOf(window.document.activeElement as HTMLButtonElement);
    const target = nextIndex(current, event.key, items.length, { orientation: "vertical", wrap: true });
    if (target === null) return;
    event.preventDefault();
    itemElements[target]?.focus();
  }

  /** where an item sits among the items, skipping separators, for focus. */
  const itemIndex = (item: DocumentMenuItem) => items.indexOf(item);

  // a click anywhere else closes the menu
  $effect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!anchor?.contains(event.target as Node)) hide(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  });
</script>

<div class="anchor" bind:this={anchor}>
  <button
    class="title-button"
    class:open
    title={`${doc.path}\n${tooltipFor("documentMenu")}`}
    aria-haspopup="menu"
    aria-expanded={open}
    bind:this={button}
    onclick={() => (open ? hide(false) : show())}
    onkeydown={onButtonKeyDown}
  >
    <span class="name">{doc.name}</span>
    {#if doc.dirty}
      <span class="edited">— Edited</span>
    {/if}
    <span class="chevron"><Icon name="chevronDown" size={0.75} /></span>
  </button>

  {#if open}
    <div class="glass menu" role="menu" aria-label={tools.documentMenu.label} tabindex="-1" onkeydown={onMenuKeyDown}>
      {#each entries as entry, position (entry === SEPARATOR ? `separator-${position}` : entry.command)}
        {#if entry === SEPARATOR}
          <div class="separator" role="separator"></div>
        {:else}
          <button
            class="item"
            role="menuitem"
            tabindex="-1"
            aria-disabled={!entry.enabled}
            title={entry.tooltip}
            bind:this={itemElements[itemIndex(entry)]}
            onclick={() => choose(entry)}
          >
            <span class="label">{entry.label}</span>
            {#if entry.shortcut}
              <kbd>{entry.shortcut}</kbd>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  /* not positioned on purpose: the title around it clips its overflow
     (so a long name can shrink), and the menu must not be clipped with
     it. the menu is placed against the toolbar group instead, whose glass
     material makes it the menu's containing block */
  .anchor {
    display: flex;
    min-width: 0;
  }

  /* looks like the bold title it always was, until pointed at. it keeps
     its full width unless the toolbar runs out of room, then the name
     shrinks with an ellipsis */
  .title-button {
    display: flex;
    align-items: baseline;
    gap: 0.25rem;
    min-width: 0;
    padding: 0.0625rem 0.25rem;
    margin-left: -0.25rem;
    border: 0;
    border-radius: 0.3125rem;
    background: transparent;
    font: var(--headline);
    text-align: left;
    transition: background 0.15s var(--ease);
  }

  .title-button:hover {
    background: var(--control-hover);
  }

  .title-button:active,
  .title-button.open {
    background: var(--control-active);
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* quieter than the name, as in a mac title bar */
  .edited {
    flex-shrink: 0;
    color: var(--label-secondary);
    font-weight: 400;
    white-space: nowrap;
  }

  .chevron {
    flex-shrink: 0;
    align-self: center;
    display: grid;
    color: var(--label-secondary);
  }

  /* hangs from the toolbar group's leading edge, under the title */
  .menu {
    position: absolute;
    top: calc(100% + 0.5rem);
    left: 0;
    z-index: 2;
    display: flex;
    flex-direction: column;
    min-width: 13rem;
    padding: var(--group-padding);
    outline: none;
    box-shadow:
      inset 0 0 0 0.5px var(--glass-edge),
      var(--shadow-glass);
    animation: drop 0.15s var(--ease);
  }

  .item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    height: 1.75rem;
    padding: 0 0.625rem;
    border: 0;
    /* concentric with the menu around it */
    border-radius: var(--radius-control);
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .item:hover:not([aria-disabled="true"]),
  .item:focus-visible {
    background: var(--control-hover);
  }

  .item:active:not([aria-disabled="true"]) {
    background: var(--control-active);
  }

  .item[aria-disabled="true"] {
    color: var(--label-tertiary);
  }

  .item[aria-disabled="true"] kbd {
    color: var(--label-tertiary);
  }

  .label {
    flex: 1;
  }

  kbd {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .separator {
    height: 0.5px;
    margin: 0.25rem 0.625rem;
    background: var(--separator);
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-0.25rem);
    }
  }
</style>
