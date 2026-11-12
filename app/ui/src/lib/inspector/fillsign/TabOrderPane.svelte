<script lang="ts">
  // Prepare Form > Order: the order Tab goes through a page's fields.
  // rows or columns are worked out by the engine; Manual is a list to
  // drag, or to move with the buttons (Alt and the arrows from the
  // keyboard). the engine also puts the page's annotations in this order,
  // for readers that ignore the page's tab setting.
  import { untrack } from "svelte";
  import { asCommandError } from "../../api";
  import { fieldsOnPage, moveItem, tabOrderEffect } from "../../forms/formBuilding";
  import { setTabOrder, tabOrder, type TabOrder } from "../../forms/formBuildingApi";
  import { fieldLabel } from "../../forms/tabOrder";
  import type { ToolContext } from "../../inspector";
  import Choices from "../Choices.svelte";

  interface Props {
    context: ToolContext;
  }

  let { context }: Props = $props();

  const session = $derived(context.fillSign!);
  let page = $state(untrack(() => context.currentPage));
  let kind = $state<TabOrder["type"]>("manual");
  let order = $state<string[]>([]);
  let dragged = $state<number | null>(null);
  const listed = $derived(fieldsOnPage(order, session.fields));

  // the page's current order, read again after every change
  $effect(() => {
    const id = context.document.id;
    const shownPage = page;
    void context.document.revision;
    untrack(() =>
      tabOrder(id, shownPage)
        .then((names) => (order = names))
        .catch((problem) => context.showError(asCommandError(problem).message)),
    );
  });

  function move(from: number, to: number) {
    order = moveItem(order, from, to);
  }

  function onKey(event: KeyboardEvent, index: number) {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    event.preventDefault();
    const to = event.key === "ArrowUp" ? index - 1 : index + 1;
    move(index, to);
    // the moved row keeps the focus, so it can go on moving
    queueMicrotask(() => (document.querySelector(`[data-order-row="${to}"]`) as HTMLElement | null)?.focus());
  }

  async function apply() {
    const id = context.document.id;
    const chosen: TabOrder = kind === "manual" ? { type: "manual", fields: [...order] } : { type: kind };
    const worked = await context.change(() => setTabOrder(id, page, chosen));
    if (worked) context.notify(`Tab order set for page ${page + 1}.`);
  }
</script>

<label class="setting">
  <span class="caption">Page</span>
  <select class="field" bind:value={page}>
    {#each context.document.pages as _, index (index)}<option value={index}>Page {index + 1}</option>{/each}
  </select>
</label>
<Choices
  label="Tab Goes"
  options={[
    { value: "rows", label: "Rows" },
    { value: "columns", label: "Columns" },
    { value: "structure", label: "Tags" },
    { value: "manual", label: "My Order" },
  ]}
  bind:value={kind}
/>
{#if kind === "manual"}
  {#if listed.length === 0}
    <p class="hint">There are no fields on page {page + 1}.</p>
  {:else}
    <ol class="order" aria-label="Fields in tab order">
      {#each listed as field, index (field.name)}
        <li
          draggable="true"
          class:dragging={dragged === index}
          ondragstart={() => (dragged = index)}
          ondragover={(event) => event.preventDefault()}
          ondrop={() => {
            if (dragged !== null) move(dragged, index);
            dragged = null;
          }}
          ondragend={() => (dragged = null)}
        >
          <span class="row" tabindex="0" data-order-row={index} role="button" aria-label="{fieldLabel(field)}, {index + 1} of {listed.length}. Alt and the arrows move it." onkeydown={(event) => onKey(event, index)}>
            <span class="number">{index + 1}</span>
            <span class="name">{fieldLabel(field)}</span>
          </span>
          <button class="small" aria-label="Move {fieldLabel(field)} up" disabled={index === 0} onclick={() => move(index, index - 1)}>↑</button>
          <button class="small" aria-label="Move {fieldLabel(field)} down" disabled={index === listed.length - 1} onclick={() => move(index, index + 1)}>↓</button>
        </li>
      {/each}
    </ol>
    <p class="hint">Drag a field, or select it and press Alt with the up and down arrows.</p>
  {/if}
{/if}
<div class="actions"><button class="button primary" onclick={apply}>Set Tab Order</button></div>
<p class="effect" aria-live="polite">{tabOrderEffect(kind, page)}</p>

<style>
  .order {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.125rem 0.25rem;
    border-radius: var(--radius-small);
    background: var(--control);
    cursor: grab;
  }

  li.dragging {
    opacity: 0.5;
  }

  .row {
    display: flex;
    flex: 1;
    gap: 0.5rem;
    min-width: 0;
  }

  .number {
    color: var(--label-secondary);
    font-variant-numeric: tabular-nums;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .small {
    border: 0;
    background: transparent;
    color: var(--label-secondary);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }

  .effect {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    line-height: 0.9375rem;
  }
</style>
