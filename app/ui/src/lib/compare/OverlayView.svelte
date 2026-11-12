<script lang="ts">
  // Overlay: one column with each page pair drawn as the engine's overlay
  // picture (shared ink grey, removed red, added green), lined up. the
  // slider fades from the old page through the overlay to the new page;
  // Flash shows the two versions one after the other. pictures are asked
  // for only for the pairs near the screen, and a few are kept.
  import { visiblePages, type PageLayout } from "../layout";
  import { pairPictures, type PagePair, type PairPictures } from "./compareApi";
  import { FLASH_MS, flashMode, flashStep, layersAt, pictureScale, sliderText } from "./overlay";
  import { rectsOn } from "./changes";
  import type { Change } from "./compareApi";

  interface Props {
    jobId: number;
    pairs: PagePair[];
    layout: PageLayout;
    zoom: number;
    pixelRatio: number;
    inset: number;
    selected: { pair: number; change: Change } | null;
    scroller?: HTMLDivElement;
  }

  let { jobId, pairs, layout, zoom, pixelRatio, inset, selected, scroller = $bindable() }: Props = $props();

  const KEEP = 8;
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let viewportWidth = $state(0);
  let position = $state(50);
  let flashing = $state(false);
  let pictures = $state<Map<string, PairPictures>>(new Map());
  const asked = new Set<string>();
  const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

  const scale = $derived(pictureScale(zoom, pixelRatio));
  const layers = $derived(layersAt(position));
  const contentWidth = $derived(Math.max(layout.widestRow + 32, viewportWidth));
  const shown = $derived.by(() => {
    const range = visiblePages(layout.pages, scrollTop, viewportHeight, viewportHeight / 3);
    return range ? layout.pages.slice(range.first, range.last + 1) : [];
  });

  // pictures for the pairs on screen, newest kept, the rest let go
  $effect(() => {
    for (const placed of shown) {
      const key = `${placed.index}@${scale}`;
      if (pictures.has(key) || asked.has(key)) continue;
      asked.add(key);
      pairPictures(jobId, placed.index, scale)
        .then((answer) => {
          const next = new Map(pictures);
          next.set(key, answer);
          while (next.size > KEEP) next.delete(next.keys().next().value!);
          pictures = next;
        })
        .catch(() => asked.delete(key));
    }
  });

  // flashing alternates the versions by itself, unless motion is reduced
  $effect(() => {
    if (!flashing || flashMode(reducedMotion) === "step") return;
    const timer = setInterval(() => (position = flashStep(position)), FLASH_MS);
    return () => clearInterval(timer);
  });

  function onFlash() {
    if (flashMode(reducedMotion) === "step") position = flashStep(position);
    else {
      flashing = !flashing;
      if (!flashing) position = 50;
    }
  }

  function selectedRects(index: number) {
    if (!selected || selected.pair !== index) return [];
    const page = pairs[index].rightPage;
    if (page !== null) {
      const rects = rectsOn(selected.change, "new", page);
      if (rects.length > 0) return rects;
    }
    const old = pairs[index].leftPage;
    return old !== null ? rectsOn(selected.change, "old", old) : [];
  }
</script>

<div class="overlay-view">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="column"
    role="region"
    aria-label="Both versions laid over each other"
    tabindex="0"
    bind:this={scroller}
    bind:clientHeight={viewportHeight}
    bind:clientWidth={viewportWidth}
    onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
  >
    <div class="content" style:width="{contentWidth}px" style:height="{layout.totalHeight}px">
      {#each shown as placed (placed.index)}
        {@const found = pictures.get(`${placed.index}@${scale}`)}
        <div
          class="sheet"
          role="img"
          aria-label={`Page pair ${placed.index + 1} of ${pairs.length}, ${sliderText(position)}`}
          style:left="{contentWidth / 2 + placed.left}px"
          style:top="{placed.top}px"
          style:width="{placed.width}px"
          style:height="{placed.height}px"
        >
          {#if found}
            <img src={found.overlay} alt="" style:opacity={layers.overlay} />
            <img src={found.old} alt="" style:opacity={layers.old} />
            <img src={found.new} alt="" style:opacity={layers.next} />
          {:else}
            <span class="drawing">Drawing…</span>
          {/if}
          {#each selectedRects(placed.index) as rect, index (index)}
            <div
              class="selected"
              style:left="{rect.x * zoom - 3}px"
              style:top="{rect.y * zoom - 3}px"
              style:width="{rect.width * zoom + 6}px"
              style:height="{rect.height * zoom + 6}px"
            ></div>
          {/each}
        </div>
      {/each}
    </div>
  </div>

  <div class="glass controls" role="group" aria-label="Overlay" style:top="{inset}px">
    <label class="blend">
      <span>Old</span>
      <input
        type="range"
        min="0"
        max="100"
        step="5"
        bind:value={position}
        aria-label="Old to New"
        aria-valuetext={sliderText(position)}
        title="Fade from the old version, through both, to the new version"
        oninput={() => (flashing = false)}
      />
      <span>New</span>
    </label>
    <button class="button" aria-pressed={flashMode(reducedMotion) === "auto" ? flashing : undefined} onclick={onFlash} title="Show the two versions one after the other">
      {flashMode(reducedMotion) === "step" ? (position >= 50 ? "Show Old" : "Show New") : flashing ? "Stop Flashing" : "Flash"}
    </button>
  </div>
  <div class="visually-hidden" aria-live="polite">{sliderText(position)}</div>
</div>

<style>
  .overlay-view {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
  }

  .column {
    height: 100%;
    overflow: auto;
    background: var(--desk);
    outline: none;
  }

  .column:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .content {
    position: relative;
  }

  .sheet {
    position: absolute;
    overflow: hidden;
    border-radius: 2px;
    background: #ffffff;
    box-shadow: var(--shadow-page);
  }

  .sheet img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    user-select: none;
  }

  .drawing {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: #6e6e73;
  }

  .selected {
    position: absolute;
    outline: 2px solid var(--accent);
    border-radius: 3px;
  }

  .controls {
    position: absolute;
    left: 50%;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin-top: 0.5rem;
    padding: 0.375rem 0.5rem 0.375rem 0.875rem;
    transform: translateX(-50%);
  }

  .blend {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  .blend input {
    width: min(14rem, 30vw);
    accent-color: var(--accent);
  }
</style>
