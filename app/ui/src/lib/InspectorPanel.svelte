<script lang="ts">
  // the Tools inspector on the trailing side: a segmented control to pick
  // a tab, and the tab's tools below it. tabs come from the registry in
  // inspector.ts and are loaded the first time they are shown.
  //
  // on a small window it floats over the document instead of narrowing
  // it (see sidePanels.ts), like a sheet with its own close button.
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { INSPECTOR_TABS, tabById, type ToolContext } from "./inspector";
  import { nextIndex } from "./keyboard";

  interface Props {
    tabId: string;
    context: ToolContext;
    floating: boolean;
    onTabChange: (id: string) => void;
    onClose: () => void;
  }

  let { tabId, context, floating, onTabChange, onClose }: Props = $props();

  const tab = $derived(tabById(tabId));
  let tabs: HTMLButtonElement[] = $state([]);

  async function onTabKeyDown(event: KeyboardEvent) {
    const current = INSPECTOR_TABS.findIndex((other) => other.id === tab.id);
    const target = nextIndex(current, event.key, INSPECTOR_TABS.length, { orientation: "horizontal", wrap: true });
    if (target === null) return;
    event.preventDefault();
    onTabChange(INSPECTOR_TABS[target].id);
    await tick();
    tabs[target]?.focus();
  }

  function onKeyDown(event: KeyboardEvent) {
    // escape closes a floating inspector, as it would a sheet
    if (event.key === "Escape" && floating && !event.defaultPrevented) {
      event.stopPropagation();
      onClose();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside class="inspector" class:floating aria-label="Tools" onkeydown={onKeyDown}>
  <header>
    <h2>Tools</h2>
    <button class="close" title="Close the tools" aria-label="Close the tools" onclick={onClose}>
      <Icon name="close" size={0.75} />
    </button>
  </header>

  <!-- shown even with one tab, so the place where later tabs appear is already familiar -->
  <div class="segments" role="tablist" aria-label="Kind of tools">
      {#each INSPECTOR_TABS as option, index (option.id)}
        <button
          id={`inspector-tab-${option.id}`}
          class="segment"
          class:selected={option.id === tab.id}
          role="tab"
          aria-selected={option.id === tab.id}
          aria-controls="inspector-panel"
          tabindex={option.id === tab.id ? 0 : -1}
          title={option.tooltip}
          bind:this={tabs[index]}
          onclick={() => onTabChange(option.id)}
          onkeydown={onTabKeyDown}
        >
          {option.label}
        </button>
      {/each}
  </div>

  <div id="inspector-panel" class="panel" role="tabpanel" aria-labelledby={`inspector-tab-${tab.id}`}>
    {#await tab.load() then module}
      <module.default {context} />
    {/await}
  </div>
</aside>

<style>
  /* the width matches PANEL_WIDTH_REM in sidePanels.ts */
  .inspector {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    width: 18rem;
    height: 100%;
    padding: 0.75rem 0.75rem 0;
    border-left: 0.5px solid var(--separator);
    background: var(--glass);
    backdrop-filter: var(--blur);
  }

  /* over the document on a small window, as wide as fits */
  .inspector.floating {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 3;
    width: min(20rem, 100%);
    box-shadow: var(--shadow-glass);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.25rem 0.25rem 0.5rem;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .close {
    display: grid;
    place-items: center;
    width: 1.5rem;
    height: 1.5rem;
    border: 0;
    border-radius: 50%;
    background: var(--control);
    color: var(--label-secondary);
  }

  .close:hover {
    background: var(--control-hover);
  }

  .segments {
    display: flex;
    flex-shrink: 0;
    margin-bottom: 0.5rem;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segment {
    flex: 1;
    min-width: 0;
    height: 1.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
    letter-spacing: var(--subheadline-spacing);
  }

  .segment.selected {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }

  .panel {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: 1rem;
  }
</style>
