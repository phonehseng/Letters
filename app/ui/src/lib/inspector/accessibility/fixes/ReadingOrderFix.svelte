<script lang="ts">
  // the reading order of one page: the page shows a numbered box on each
  // block, and this list says them in the order a screen reader will read
  // them. drag a block, or press Alt with the up or down arrow on it (or
  // use its buttons), then apply.
  import { readingBlocks, type ObjectRef, type ReadingBlock } from "../../../a11y/a11yApi";
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import { moveItem, movedText } from "../../../a11y/ladder";
  import type { ToolContext } from "../../../inspector";

  interface Props {
    context: ToolContext;
    session: A11ySession;
    page: number;
    proposal: ObjectRef[];
  }

  let { context, session, page, proposal }: Props = $props();

  let blocks = $state<ReadingBlock[]>([]);
  let loaded = $state(false);
  let busy = $state(false);
  let status = $state("");
  let dragging = $state<number | null>(null);
  let items: HTMLButtonElement[] = $state([]);

  $effect(() => {
    readingBlocks(context.document.id, page, proposal)
      .then((found) => {
        blocks = found;
        session.showReadingOrder(page, found);
        session.clearSpot();
        session.revealPage(page, found[0]?.rect ?? null);
      })
      .catch(() => (blocks = []))
      .finally(() => (loaded = true));
    return () => session.hideReadingOrder();
  });

  // the page shows the numbers in the list's order as it changes
  $effect(() => {
    if (loaded) session.showReadingOrder(page, blocks);
  });

  function move(from: number, to: number) {
    const next = moveItem(blocks, from, to);
    if (next === blocks) return;
    blocks = next;
    const landed = Math.min(blocks.length - 1, Math.max(0, to));
    status = movedText(landed, blocks.length);
    queueMicrotask(() => items[landed]?.focus());
  }

  function onKeyDown(event: KeyboardEvent, index: number) {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    event.preventDefault();
    event.stopPropagation();
    move(index, index + (event.key === "ArrowUp" ? -1 : 1));
  }

  async function apply() {
    busy = true;
    await session.fix({ kind: "setReadingOrder", page, order: blocks.map((block) => block.element) });
    busy = false;
  }

  function name(block: ReadingBlock): string {
    const role = block.role === "P" ? "Paragraph" : block.role === "Figure" ? "Picture" : block.role;
    return block.text ? `${role}: ${block.text}` : role;
  }
</script>

<p class="hint">{session.words?.readingOrder ?? ""} The numbers on page {page + 1} follow this list.</p>
{#if !loaded}
  <p class="hint">Reading the page…</p>
{:else}
  <ol class="order" aria-label={`Reading order of page ${page + 1}`}>
    {#each blocks as block, index (`${block.element.number}.${block.element.generation}`)}
      <!-- dragging is for the pointer; the keyboard moves a block with Alt and the arrows, or its buttons -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <li
        draggable="true"
        class:dragging={dragging === index}
        ondragstart={() => (dragging = index)}
        ondragover={(event) => event.preventDefault()}
        ondrop={() => {
          if (dragging !== null) move(dragging, index);
          dragging = null;
        }}
        ondragend={() => (dragging = null)}
      >
        <span class="number" aria-hidden="true">{index + 1}</span>
        <!-- pressing a block shows it on the page -->
        <button
          class="text"
          aria-label={`${index + 1} of ${blocks.length}. ${name(block)}. Alt with the up or down arrow moves it.`}
          bind:this={items[index]}
          onkeydown={(event) => onKeyDown(event, index)}
          onclick={() => session.showSpot(page, block.rect, name(block))}
        >
          {name(block)}
        </button>
        <span class="moves">
          <button class="button small" aria-label={`Move “${block.text || block.role}” up`} disabled={index === 0} onclick={() => move(index, index - 1)}>↑</button>
          <button class="button small" aria-label={`Move “${block.text || block.role}” down`} disabled={index === blocks.length - 1} onclick={() => move(index, index + 1)}>↓</button>
        </span>
      </li>
    {/each}
  </ol>
  <div class="visually-hidden" role="status" aria-live="polite">{status}</div>
  <div class="actions">
    <button class="button primary" disabled={blocks.length === 0 || busy} onclick={apply}>Apply Order</button>
  </div>
  <p class="hint">Changes only the order screen readers use; the page looks the same. Undo with Ctrl Z.</p>
{/if}

<style>
  .order {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    min-height: 2.25rem;
    padding: 0.25rem 0.375rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  li.dragging {
    opacity: 0.5;
  }

  .number {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    min-width: 1.25rem;
    height: 1.25rem;
    border-radius: 0.625rem;
    background: rgb(90 40 180);
    color: #ffffff;
    font: var(--subheadline);
    font-weight: 700;
  }

  .text {
    flex: 1;
    min-width: 0;
    min-height: 1.75rem;
    padding: 0 0.25rem;
    overflow: hidden;
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .moves {
    display: flex;
    gap: 0.125rem;
  }

  .small {
    min-width: 1.75rem;
    padding: 0.125rem 0.375rem;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
