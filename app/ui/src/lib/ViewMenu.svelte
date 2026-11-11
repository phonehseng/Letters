<script lang="ts">
  // the "View" pull-down: how pages are shown. two on/off options live
  // here instead of as two more toolbar buttons, so the toolbar stays at
  // three short groups (Preview and Pages do the same with a View menu).
  //
  // it behaves like a desktop menu: arrows move, Enter or Space choose,
  // Escape closes and gives focus back to the button.
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { nextIndex } from "./keyboard";
  import ToolButton from "./ToolButton.svelte";
  import { tools, type ToolHelp, type ToolName } from "./tools";

  interface Props {
    twoPages: boolean;
    nightMode: boolean;
    onToggleTwoPages: () => void;
    onToggleNightMode: () => void;
    hideLabel?: boolean;
  }

  let { twoPages, nightMode, onToggleTwoPages, onToggleNightMode, hideLabel = false }: Props = $props();

  let open = $state(false);
  let button = $state<HTMLButtonElement>();
  let anchor = $state<HTMLDivElement>();
  let items: HTMLButtonElement[] = $state([]);

  const options = $derived<{ name: ToolName; help: ToolHelp; checked: boolean; toggle: () => void }[]>([
    { name: "twoPages", help: tools.twoPages, checked: twoPages, toggle: onToggleTwoPages },
    { name: "nightMode", help: tools.nightMode, checked: nightMode, toggle: onToggleNightMode },
  ]);

  async function show() {
    open = true;
    await tick();
    items[0]?.focus();
  }

  function hide(giveFocusBack: boolean) {
    open = false;
    if (giveFocusBack) button?.focus();
  }

  function choose(toggle: () => void) {
    toggle();
    hide(true);
  }

  function onButtonKeyDown(event: KeyboardEvent) {
    // the down arrow opens a pull-down, as on every desktop system
    if (event.key !== "ArrowDown" || open) return;
    event.preventDefault();
    show();
  }

  function onMenuKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      // escape belongs to the menu; the find bar must not also close
      event.stopPropagation();
      hide(true);
      return;
    }
    if (event.key === "Tab") {
      hide(false);
      return;
    }
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const target = nextIndex(current, event.key, items.length, { orientation: "vertical", wrap: true });
    if (target === null) return;
    event.preventDefault();
    items[target].focus();
  }

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
  <ToolButton
    name="view"
    icon="view"
    {hideLabel}
    menuOpen={open}
    bind:element={button}
    onClick={() => (open ? hide(false) : show())}
    onKeyDown={onButtonKeyDown}
  />

  {#if open}
    <div class="glass menu" role="menu" aria-label={tools.view.label} tabindex="-1" onkeydown={onMenuKeyDown}>
      {#each options as option, index (option.name)}
        <button
          class="item"
          role="menuitemcheckbox"
          aria-checked={option.checked}
          tabindex="-1"
          title={option.help.tooltip}
          bind:this={items[index]}
          onclick={() => choose(option.toggle)}
        >
          <span class="check">
            {#if option.checked}<Icon name="check" size={0.875} />{/if}
          </span>
          <span class="label">{option.help.label}</span>
          {#if option.help.shortcut}
            <kbd>{option.help.shortcut}</kbd>
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .anchor {
    position: relative;
  }

  /* hangs from the button's trailing edge so it never runs off the window */
  .menu {
    position: absolute;
    top: calc(100% + 0.5rem);
    right: calc(-1 * var(--group-padding));
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
    gap: 0.375rem;
    height: 1.75rem;
    padding: 0 0.625rem 0 0.375rem;
    border: 0;
    /* concentric with the menu around it */
    border-radius: var(--radius-control);
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .item:hover,
  .item:focus-visible {
    background: var(--control-hover);
  }

  .item:active {
    background: var(--control-active);
  }

  .check {
    display: grid;
    place-items: center;
    width: 1rem;
    color: var(--accent-label);
  }

  .label {
    flex: 1;
  }

  kbd {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-0.25rem);
    }
  }
</style>
