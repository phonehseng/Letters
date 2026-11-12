<script lang="ts">
  // the changes on one page of one side: soft tinted boxes in the kind's
  // quiet colour, each with a small word ("Added", "Moved") so colour is
  // never the only sign. a click picks the change and shows its partner
  // on the other side. the list in the sidebar is the way in for the
  // keyboard and screen readers, so the marks themselves are hidden from them.
  import type { Change } from "./compareApi";
  import { markStyle, rectsOn, type Side } from "./changes";

  interface Props {
    side: Side;
    /** the side's page these marks are on */
    page: number;
    changes: Change[];
    /** css pixels per point */
    scale: number;
    pageSize: { width: number; height: number };
    selectedId: number | null;
    onSelect: (id: number) => void;
  }

  let { side, page, changes, scale, pageSize, selectedId, onSelect }: Props = $props();

  /** more changes than this on one page and only the selected one is labelled */
  const CROWDED = 24;

  function boxesOf(change: Change) {
    const rects = rectsOn(change, side, page);
    // a whole page added, removed or moved
    return rects.length > 0 ? rects : [{ x: 0, y: 0, width: pageSize.width, height: pageSize.height }];
  }
</script>

<div class="marks" aria-hidden="true">
  {#each changes as change (change.id)}
    {@const style = markStyle(change.kind, side)}
    {@const selected = change.id === selectedId}
    {#each boxesOf(change) as rect, index (index)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div
        class="mark {style.tone}"
        class:selected
        data-change={change.id}
        title={`${style.label}: ${change.detail}`}
        style:left="{rect.x * scale - 2}px"
        style:top="{rect.y * scale - 2}px"
        style:width="{rect.width * scale + 4}px"
        style:height="{rect.height * scale + 4}px"
        onclick={() => onSelect(change.id)}
      >
        <!-- on a crowded page only the selected change keeps its word, so the words don't cover the text -->
        {#if index === 0 && (selected || changes.length <= CROWDED)}
          <span class="label">{style.label}</span>
        {/if}
      </div>
    {/each}
  {/each}
</div>

<style>
  .marks {
    position: absolute;
    inset: 0;
  }

  .mark {
    position: absolute;
    border-radius: 3px;
    cursor: pointer;
  }

  /* quiet tints, multiplied so the words under them stay readable */
  .removed {
    background: rgb(255 59 48 / 0.18);
    box-shadow: inset 0 0 0 1px rgb(215 0 21 / 0.55);
  }

  .added {
    background: rgb(52 199 89 / 0.2);
    box-shadow: inset 0 0 0 1px rgb(0 130 50 / 0.55);
  }

  .moved {
    background: rgb(175 82 222 / 0.16);
    box-shadow: inset 0 0 0 1px rgb(130 50 180 / 0.55);
  }

  .formatting {
    background: rgb(255 204 0 / 0.22);
    box-shadow: inset 0 0 0 1px rgb(160 110 0 / 0.6);
  }

  .layout {
    background: rgb(48 176 199 / 0.16);
    box-shadow: inset 0 0 0 1px rgb(0 120 140 / 0.55);
  }

  .picture {
    background: rgb(255 149 0 / 0.14);
    box-shadow: inset 0 0 0 1px rgb(180 90 0 / 0.55);
  }

  /* the selected change and its partner: a firm outline, still its own colour */
  .mark.selected {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* the word on the mark: dark text on a light chip, so it reads on any page */
  .label {
    position: absolute;
    top: -1.1rem;
    left: 0;
    padding: 0 0.25rem;
    border-radius: 0.25rem;
    background: rgb(255 255 255 / 0.95);
    box-shadow: 0 0 0 0.5px rgb(0 0 0 / 0.25);
    color: #1d1d1f;
    font: var(--footnote);
    font-weight: 600;
    white-space: nowrap;
    pointer-events: none;
  }

  @media (prefers-contrast: more) {
    .mark {
      box-shadow: inset 0 0 0 2px currentColor;
    }
  }
</style>
