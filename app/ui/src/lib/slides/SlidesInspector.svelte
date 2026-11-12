<script lang="ts">
  // the slide editor's inspector on the trailing side, like keynote's
  // Format inspector: Slide (layout, hiding, what the file keeps),
  // Arrange (the selected object's place, size, angle, stacking and fill,
  // and a table's rows and columns) and Theme (colours and fonts).
  import { tick } from "svelte";
  import Icon from "../Icon.svelte";
  import { nextIndex } from "../keyboard";
  import { tools } from "../tools";
  import type { DeckSession } from "./deckSession.svelte";
  import ArrangeTab from "./inspector/ArrangeTab.svelte";
  import SlideTab from "./inspector/SlideTab.svelte";
  import ThemeTab from "./inspector/ThemeTab.svelte";

  export type SlidesTab = "slide" | "arrange" | "theme";

  interface Props {
    session: DeckSession;
    tab: SlidesTab;
    floating: boolean;
    onTab: (tab: SlidesTab) => void;
    onClose: () => void;
    onAnnounce: (words: string) => void;
  }

  let { session, tab, floating, onTab, onClose, onAnnounce }: Props = $props();

  const TABS: { id: SlidesTab; label: string; tooltip: string }[] = [
    { id: "slide", label: "Slide", tooltip: "This slide’s layout, and whether the slide show skips it" },
    { id: "arrange", label: tools.sArrange.label, tooltip: tools.sArrange.tooltip },
    { id: "theme", label: tools.sTheme.label, tooltip: tools.sTheme.tooltip },
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
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside
  class="inspector"
  class:floating
  aria-label={tools.sFormat.label}
  onkeydown={(event) => {
    if (event.key === "Escape" && !event.defaultPrevented) {
      event.stopPropagation();
      onClose();
    }
  }}
>
  <header>
    <h2>{tools.sFormat.label}</h2>
    <button class="close" title="Close the inspector (Esc)" aria-label="Close the inspector" onclick={onClose}>
      <Icon name="close" size={0.75} />
    </button>
  </header>
  <div class="segments" role="tablist" aria-label="Inspector tabs">
    {#each TABS as option, index (option.id)}
      <button
        id={`slides-tab-${option.id}`}
        class="segment"
        class:selected={option.id === tab}
        role="tab"
        aria-selected={option.id === tab}
        aria-controls="slides-panel"
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
  <div id="slides-panel" class="panel" role="tabpanel" aria-labelledby={`slides-tab-${tab}`}>
    {#if tab === "slide"}
      <SlideTab {session} {onAnnounce} />
    {:else if tab === "arrange"}
      <ArrangeTab {session} {onAnnounce} />
    {:else}
      <ThemeTab {session} {onAnnounce} />
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
    box-sizing: border-box;
    border-left: 0.5px solid var(--separator);
    background: var(--glass);
    backdrop-filter: var(--blur);
  }

  .inspector.floating {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 6;
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

  .panel :global(section) {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem 0.25rem;
    border-bottom: 0.5px solid var(--separator);
  }

  .panel :global(h3) {
    margin: 0;
    font: var(--headline);
  }
</style>
