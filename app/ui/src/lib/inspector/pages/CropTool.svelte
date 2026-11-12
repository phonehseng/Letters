<script lang="ts">
  // crops pages to a rectangle drawn on the page being read. while this
  // section is open the rectangle shows on that page with handles; the
  // number fields here and the handles there change the same rectangle.
  import { untrack } from "svelte";
  import type { ToolContext } from "../../inspector";
  import { cropPages, uncropPages } from "../../pageApi";
  import { coversPage, keepOnPage, roundPoints, type CropBox } from "../../cropRect";
  import { cropEffect } from "../../pageEffects";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  // the rectangle is drawn on the page only while the tool is open. only
  // `open` is watched: the context changes on every scroll, and re-running
  // this then would throw the rectangle away
  $effect(() => {
    const active = open;
    untrack(() => context.crop.setActive(active));
    return () => untrack(() => context.crop.setActive(false));
  });

  let applyTo = $state<"page" | "target" | "all">("page");
  const count = $derived(context.document.pages.length);
  const page = $derived(context.document.pages[context.currentPage]);
  const box = $derived(context.crop.box);
  const pages = $derived(
    applyTo === "all"
      ? Array.from({ length: count }, (_, index) => index)
      : applyTo === "target"
        ? context.target.pages
        : [context.currentPage],
  );
  const outcome = $derived(
    box && page && coversPage(box, page) ? "Drag the rectangle’s handles in to choose what to keep." : cropEffect(pages, count),
  );

  const FIELDS: { key: keyof CropBox; label: string }[] = [
    { key: "left", label: "Left" },
    { key: "top", label: "Top" },
    { key: "width", label: "Width" },
    { key: "height", label: "Height" },
  ];

  function setField(key: keyof CropBox, value: number) {
    if (!box || !page || !Number.isFinite(value)) return;
    context.crop.setBox(keepOnPage({ ...box, [key]: value }, page));
  }

  function crop() {
    if (!box) return;
    const rect = { ...box };
    return context.change(() => cropPages(context.document.id, { pages, rect }));
  }
</script>

<ToolSection
  tool="cropPages"
  {open}
  {onToggle}
  actionLabel="Crop"
  effect={outcome}
  ready={box !== null && page !== undefined && !coversPage(box, page)}
  onAction={crop}
>
  <p class="hint">Drag the rectangle on page {context.currentPage + 1}, or type its place in points (72 points make an inch).</p>
  {#if box}
    <div class="setting-row">
      {#each FIELDS as field (field.key)}
        <label class="setting">
          <span class="caption">{field.label}</span>
          <input
            class="field"
            type="number"
            step="1"
            value={roundPoints(box[field.key])}
            onchange={(event) => setField(field.key, Number(event.currentTarget.value))}
          />
        </label>
      {/each}
    </div>
  {/if}
  <Choices
    label="Apply To"
    options={[
      { value: "page", label: "This Page" },
      { value: "target", label: "Selected", disabled: !context.target.fromSelection, title: "Select pages in the sidebar first" },
      { value: "all", label: "All Pages" },
    ]}
    bind:value={applyTo}
  />
  {#snippet secondary()}
    <button class="button" title="Show the whole of these pages again" onclick={() => context.change(() => uncropPages(context.document.id, pages))}>
      Show Whole Page
    </button>
  {/snippet}
</ToolSection>
