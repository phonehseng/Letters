<script lang="ts" module>
  // a toolbar pull-down menu (Document, Style, Insert, View): a button that
  // opens a short list of commands, each with its shortcut, for mouse and keyboard.
  export interface PullDownItem {
    label: string;
    run: () => void;
    shortcut?: string;
    title?: string;
    disabled?: boolean;
    checked?: boolean;
    separatorBefore?: boolean;
  }
</script>

<script lang="ts">
  // a toolbar button that opens a short menu below it, like the View menu:
  // arrow keys move, Enter picks, Escape closes and gives the focus back.
  import { tick, type Snippet } from "svelte";
  import WriterIcon, { type WriterIconName } from "./WriterIcon.svelte";

  interface Props {
    label: string;
    tooltip: string;
    icon: WriterIconName;
    items: PullDownItem[];
    hideLabel?: boolean;
    /** extra content above the items, such as the table grid */
    extra?: Snippet<[() => void]>;
    /** the menu's own name for screen readers */
    menuLabel?: string;
    /** shows the button as the document's title instead of a symbol */
    title?: { name: string; edited: boolean };
  }

  let { label, tooltip, icon, items, hideLabel = false, extra, menuLabel, title }: Props = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement>();
  let menu = $state<HTMLDivElement>();

  async function show() {
    open = true;
    await tick();
    menu?.querySelector<HTMLElement>("[role=menuitem]:not([aria-disabled=true]), [data-autofocus]")?.focus();
  }

  function close(returnFocus = true) {
    open = false;
    if (returnFocus) button?.focus();
  }

  function pick(item: PullDownItem) {
    if (item.disabled) return;
    close(false);
    item.run();
  }

  function onMenuKey(event: KeyboardEvent) {
    const entries = [...(menu?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];
    const index = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      entries[(index + step + entries.length) % entries.length]?.focus();
    } else if (event.key === "Tab") {
      close(false);
    }
  }
</script>

<svelte:window onpointerdown={(event) => open && !menu?.contains(event.target as Node) && !button?.contains(event.target as Node) && close(false)} />

<div class="pull-down">
  <button
    class="tool"
    class:open
    class:symbol-only={hideLabel}
    class:title-button={!!title}
    title={tooltip}
    aria-haspopup="menu"
    aria-expanded={open}
    bind:this={button}
    onclick={() => (open ? close() : show())}
    onkeydown={(event) => event.key === "ArrowDown" && (event.preventDefault(), show())}
  >
    {#if title}
      <span class="title-name">{title.name}</span>
      {#if title.edited}<span class="title-edited">— Edited</span>{/if}
    {:else}
      <WriterIcon name={icon} />
      <span class:visually-hidden={hideLabel}>{label}</span>
    {/if}
  </button>
  {#if open}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="glass menu" role="menu" aria-label={menuLabel ?? label} tabindex="-1" bind:this={menu} onkeydown={onMenuKey}>
      {#if extra}{@render extra(() => close(false))}{/if}
      {#each items as item (item.label)}
        {#if item.separatorBefore}<div class="separator" role="separator"></div>{/if}
        <button
          class="item"
          role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
          aria-checked={item.checked}
          aria-disabled={item.disabled}
          title={item.title}
          tabindex="-1"
          onclick={() => pick(item)}
        >
          <span class="check" aria-hidden="true">{#if item.checked}<WriterIcon name="check" size={0.875} />{/if}</span>
          <span class="item-label">{item.label}</span>
          {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pull-down {
    position: relative;
  }

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

  .tool.title-button {
    flex-direction: row;
    gap: 0.375rem;
    max-width: 22rem;
    height: 2.25rem;
    padding: 0 0.625rem;
  }

  .title-name {
    overflow: hidden;
    color: var(--label) !important;
    font: var(--headline);
    text-overflow: ellipsis;
  }

  .title-edited {
    font: var(--body);
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

  .tool:active,
  .tool.open {
    background: var(--control-active);
  }

  .menu {
    position: absolute;
    top: calc(100% + 0.375rem);
    left: 0;
    z-index: 5;
    min-width: 13rem;
    padding: 0.3125rem;
    outline: none;
  }

  .item {
    display: grid;
    grid-template-columns: 1rem 1fr auto;
    align-items: center;
    gap: 0.375rem;
    width: 100%;
    padding: 0.25rem 0.5rem 0.25rem 0.25rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .item:hover,
  .item:focus-visible {
    outline: none;
    box-shadow: none;
    background: var(--accent);
    color: var(--on-accent);
  }

  .item[aria-disabled="true"] {
    color: var(--label-tertiary);
  }

  .item[aria-disabled="true"]:hover {
    background: transparent;
    color: var(--label-tertiary);
  }

  kbd {
    color: inherit;
    opacity: 0.6;
    font: var(--subheadline);
  }

  .separator {
    height: 0.5px;
    margin: 0.25rem 0.5rem;
    background: var(--separator);
  }
</style>
