<script lang="ts">
  // what the Accessibility tab outlines on one page: the spot of the
  // finding picked in the list, and during the reading order step a
  // numbered box on each block, in the order a screen reader will read
  // them. only drawn on screen; the file is not changed. the viewer puts
  // one of these over every page on screen.
  import type { PageFrame } from "../pageTools/pagePoints";
  import type { A11ySession } from "./a11ySession.svelte";

  interface Props {
    session: A11ySession;
    frame: PageFrame;
  }

  let { session, frame }: Props = $props();

  const spot = $derived(session.spot?.page === frame.index ? session.spot : null);
  const blocks = $derived(session.readingOrder?.page === frame.index ? session.readingOrder.blocks : []);
</script>

{#if spot || blocks.length > 0}
  <div
    class="a11y-marks"
    aria-hidden="true"
    style:left="{frame.left}px"
    style:top="{frame.top}px"
    style:width="{frame.width}px"
    style:height="{frame.height}px"
  >
    {#each blocks as block, index (`${block.element.number}-${index}`)}
      {#if block.rect}
        <div
          class="block"
          style:left="{block.rect.x * frame.scale}px"
          style:top="{block.rect.y * frame.scale}px"
          style:width="{block.rect.width * frame.scale}px"
          style:height="{block.rect.height * frame.scale}px"
        >
          <span class="number">{index + 1}</span>
        </div>
      {/if}
    {/each}
    {#if spot}
      <div
        class="spot"
        style:left="{spot.rect.x * frame.scale - 4}px"
        style:top="{spot.rect.y * frame.scale - 4}px"
        style:width="{spot.rect.width * frame.scale + 8}px"
        style:height="{spot.rect.height * frame.scale + 8}px"
      ></div>
    {/if}
  </div>
{/if}

<style>
  .a11y-marks {
    position: absolute;
    pointer-events: none;
  }

  /* the spot: a thick outline in the accent, which here means "this one" */
  .spot {
    position: absolute;
    border: 3px solid var(--accent);
    border-radius: 4px;
    box-shadow: 0 0 0 4px var(--accent-soft);
  }

  .block {
    position: absolute;
    border: 1.5px dashed rgb(110 60 200);
    border-radius: 3px;
    background: rgb(110 60 200 / 0.06);
  }

  /* the number says the order in words a sighted reader can count */
  .number {
    position: absolute;
    top: -0.625rem;
    left: -0.625rem;
    display: grid;
    place-items: center;
    min-width: 1.25rem;
    height: 1.25rem;
    padding: 0 0.25rem;
    border-radius: 0.625rem;
    background: rgb(90 40 180);
    color: #ffffff;
    font: var(--subheadline);
    font-weight: 700;
  }

  @media (prefers-contrast: more) {
    .block {
      border-style: solid;
      border-width: 2px;
    }
  }
</style>
