<script lang="ts">
  // the writer's inspector on the trailing side, like Pages' Format and
  // Document inspectors: Format follows the selection (text, table or
  // picture), Document holds the page, presets and typing switches, and
  // Review holds tracked changes and comments.
  import { tick } from "svelte";
  import Icon from "../Icon.svelte";
  import { nextIndex } from "../keyboard";
  import { tools } from "../tools";
  import DocumentTab from "./inspector/DocumentTab.svelte";
  import FormatTab from "./inspector/FormatTab.svelte";
  import ReviewTab from "./inspector/ReviewTab.svelte";
  import type { WriterTab } from "./explainList";
  import type { LengthUnit } from "./units";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    tab: WriterTab;
    unit: LengthUnit;
    floating: boolean;
    focusComment?: string | null;
    onTab: (tab: WriterTab) => void;
    onClose: () => void;
  }

  let { session, tab, unit, floating, focusComment = null, onTab, onClose }: Props = $props();

  const TABS: { id: WriterTab; label: string; tooltip: string }[] = [
    { id: "format", label: "Format", tooltip: "Text, paragraph, table and picture settings for the selection" },
    { id: "document", label: tools.wDocument.label, tooltip: tools.wDocument.tooltip },
    { id: "review", label: tools.wReview.label, tooltip: tools.wReview.tooltip },
  ];

  let buttons: HTMLButtonElement[] = $state([]);

  async function onTabKey(event: KeyboardEvent) {
    const index = TABS.findIndex((each) => each.id === tab);
    const target = nextIndex(index, event.key, TABS.length, { orientation: "horizontal", wrap: true });
    if (target === null) return;
    event.preventDefault();
    onTab(TABS[target].id);
    await tick();
    buttons[target]?.focus();
  }

  // the inspector takes the focus when it opens, so keyboard users land in it
  $effect(() => {
    tick().then(() => buttons[TABS.findIndex((each) => each.id === tab)]?.focus());
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside
  class="inspector"
  class:floating
  aria-label="Format"
  onkeydown={(event) => {
    if (event.key === "Escape" && !event.defaultPrevented) {
      event.stopPropagation();
      onClose();
    }
  }}
>
  <header>
    <h2>{tools.wFormat.label}</h2>
    <button class="close" title="Close the inspector (Esc)" aria-label="Close the inspector" onclick={onClose}>
      <Icon name="close" size={0.75} />
    </button>
  </header>
  <div class="segments" role="tablist" aria-label="Inspector tabs">
    {#each TABS as option, index (option.id)}
      <button
        id={`writer-tab-${option.id}`}
        class="segment"
        class:selected={option.id === tab}
        role="tab"
        aria-selected={option.id === tab}
        aria-controls="writer-panel"
        tabindex={option.id === tab ? 0 : -1}
        title={option.tooltip}
        bind:this={buttons[index]}
        onclick={() => onTab(option.id)}
        onkeydown={onTabKey}
      >
        {option.label}
      </button>
    {/each}
  </div>
  <div id="writer-panel" class="panel" role="tabpanel" aria-labelledby={`writer-tab-${tab}`}>
    {#if tab === "format"}
      <FormatTab {session} {unit} />
    {:else if tab === "document"}
      <DocumentTab {session} {unit} />
    {:else}
      <ReviewTab {session} {focusComment} />
    {/if}
  </div>
</aside>

<style>
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

  .segments {
    display: flex;
    flex-shrink: 0;
    margin-bottom: 0.25rem;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segment {
    flex: 1;
    height: 1.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
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
    padding-bottom: 3rem;
  }
</style>
