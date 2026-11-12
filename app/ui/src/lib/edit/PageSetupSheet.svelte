<script lang="ts">
  // Page Setup for one page: margins, paper size, extra space and the five
  // page boxes, each part with its own Apply (one undo step each) and a
  // plain sentence about what it does. numbers are points, 72 to the inch,
  // with the inches beside them. Escape or Done closes it.
  import { onMount } from "svelte";
  import Choices from "../inspector/Choices.svelte";
  import type { BoxNumbers } from "./contentApi";
  import type { PageSetup } from "./pageSetup.svelte";
  import { BOXES, inches, MARGIN_FITS, PAPER_FITS, PAPER_SIZES, paperNamed, SIDES, spaceSentence, type BoxName } from "./pageSetupWords";

  interface Props {
    setup: PageSetup;
  }

  let { setup }: Props = $props();

  let sheet = $state<HTMLDivElement>();
  const paperId = $derived(paperNamed(setup.paper.width, setup.paper.height)?.id ?? "custom");
  const content = $derived(setup.layout?.margins.content ?? null);

  function choosePaper(id: string) {
    const paper = PAPER_SIZES.find((each) => each.id === id);
    if (paper) setup.paper = { width: paper.width, height: paper.height };
  }

  /** the box's four numbers as typed, or its paper's when it has none of its own */
  function numbersOf(name: BoxName): BoxNumbers {
    return setup.boxes[name] ?? setup.boxes.media ?? [0, 0, setup.paper.width, setup.paper.height];
  }

  function setNumber(name: BoxName, index: number, value: number) {
    const numbers = [...numbersOf(name)] as BoxNumbers;
    numbers[index] = value;
    setup.boxes = { ...setup.boxes, [name]: numbers };
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setup.close();
    }
  }

  onMount(() => sheet?.focus());
</script>

<div class="backdrop">
  <div class="glass sheet" role="dialog" aria-modal="true" aria-labelledby="page-setup-title" tabindex="-1" data-page-setup bind:this={sheet} onkeydown={onKeyDown}>
    <h2 id="page-setup-title">Page Setup: Page {setup.page + 1}</h2>
    <p class="hint">Numbers are in points: 72 points make an inch. Each Apply is one change you can undo.</p>

    <section>
      <h3>Margins</h3>
      <p class="hint">The space between the content and each edge{content ? "" : " (this page is empty)"}.</p>
      <div class="grid four">
        {#each ["top", "right", "bottom", "left"] as const as side (side)}
          <label class="field">
            <span class="caption">{side[0].toUpperCase() + side.slice(1)}</span>
            <input type="number" step="1" bind:value={setup.margins[side]} />
            <span class="hint">{inches(setup.margins[side])}</span>
          </label>
        {/each}
      </div>
      <Choices label="Content" options={MARGIN_FITS} bind:value={setup.marginFit} />
      <p class="hint">{MARGIN_FITS.find((fit) => fit.value === setup.marginFit)?.sentence}</p>
      <div class="actions"><button class="button" data-apply="margins" onclick={() => setup.applyMargins()}>Apply Margins</button></div>
    </section>

    <section>
      <h3>Paper Size</h3>
      <label class="field">
        <span class="caption">Paper</span>
        <select value={paperId} onchange={(event) => choosePaper(event.currentTarget.value)}>
          {#each PAPER_SIZES as paper (paper.id)}<option value={paper.id}>{paper.label}</option>{/each}
          <option value="custom" disabled>Custom</option>
        </select>
      </label>
      <div class="grid two">
        <label class="field"><span class="caption">Width</span><input type="number" min="72" bind:value={setup.paper.width} /><span class="hint">{inches(setup.paper.width)}</span></label>
        <label class="field"><span class="caption">Height</span><input type="number" min="72" bind:value={setup.paper.height} /><span class="hint">{inches(setup.paper.height)}</span></label>
      </div>
      <Choices label="Content" options={PAPER_FITS} bind:value={setup.paperFit} />
      <p class="hint">{PAPER_FITS.find((fit) => fit.value === setup.paperFit)?.sentence} The page's other boxes are removed, since they described the old paper.</p>
      <div class="actions"><button class="button" data-apply="paper" onclick={() => setup.applyPaper()}>Apply Paper Size</button></div>
    </section>

    <section>
      <h3>Extra Space</h3>
      <div class="grid two">
        <label class="field">
          <span class="caption">Side</span>
          <select bind:value={setup.spaceSide}>{#each SIDES as side (side.value)}<option value={side.value}>{side.label}</option>{/each}</select>
        </label>
        <label class="field"><span class="caption">Points</span><input type="number" step="1" bind:value={setup.spacePoints} /></label>
      </div>
      <p class="hint">{spaceSentence(setup.spaceSide, setup.spacePoints)}</p>
      <div class="actions"><button class="button" data-apply="space" onclick={() => setup.applySpace()}>Apply Space</button></div>
    </section>

    <section>
      <h3>Page Boxes</h3>
      <p class="hint">Left, bottom, right and top edges, from the page's bottom left corner as the file stores it.</p>
      {#each BOXES as box (box.name)}
        <div class="box">
          <span class="caption">{box.label}</span>
          <span class="hint">{box.meaning}{setup.boxes[box.name] ? "" : " (not set: the paper's edges apply)"}</span>
          <div class="grid four">
            {#each numbersOf(box.name) as value, index (index)}
              <input type="number" aria-label={`${box.label}, ${["left", "bottom", "right", "top"][index]} edge`} {value} onchange={(event) => setNumber(box.name, index, Number(event.currentTarget.value))} />
            {/each}
          </div>
          <div class="actions"><button class="button" data-apply={`box-${box.name}`} onclick={() => setup.applyBox(box.name, numbersOf(box.name))}>Set</button></div>
        </div>
      {/each}
    </section>

    <div class="buttons"><button class="button primary" onclick={() => setup.close()}>Done</button></div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 10;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    background: rgb(0 0 0 / 0.12);
  }

  .sheet {
    width: min(32rem, calc(100% - 2rem));
    max-height: calc(100vh - var(--toolbar-space) - 2rem);
    margin-top: var(--toolbar-space);
    padding: 1.25rem 1.25rem 1rem;
    overflow: auto;
    background: var(--window);
    outline: none;
  }

  h2,
  h3 {
    margin: 0;
    font: var(--headline);
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.875rem 0;
    border-top: 0.5px solid var(--separator);
  }

  .grid {
    display: grid;
    gap: 0.5rem;
  }

  .grid.four {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  .grid.two {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
  }

  input,
  select {
    min-width: 0;
  }

  .box {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .hint {
    margin: 0;
  }

  .actions,
  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>
