<script lang="ts">
  // numbers the pages. the common case (a number at the bottom centre of
  // every page) is one click.
  import type { ToolContext } from "../../inspector";
  import { addPageNumbers, type HorizontalPlace, type NumberFormat, type PageNumberSettings, type VerticalEdge } from "../../pageApi";
  import { pageNumbersEffect } from "../../pageEffects";
  import PagesChoice from "../PagesChoice.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let place = $state("bottom center");
  let format = $state<NumberFormat>("number");
  let startNumber = $state(1);
  let skipFirstPage = $state(false);
  let applyTo = $state<"all" | "target">("all");

  const PLACES = [
    ["bottom center", "Bottom Centre"],
    ["bottom right", "Bottom Right"],
    ["bottom left", "Bottom Left"],
    ["top center", "Top Centre"],
    ["top right", "Top Right"],
    ["top left", "Top Left"],
  ];
  const FORMATS: [NumberFormat, string][] = [
    ["number", "1, 2, 3"],
    ["pageWord", "Page 1, Page 2"],
    ["ofTotal", "1 of 30"],
    ["roman", "i, ii, iii"],
  ];

  const count = $derived(context.document.pages.length);
  const settings = $derived.by((): PageNumberSettings => {
    const [vertical, horizontal] = place.split(" ") as [VerticalEdge, HorizontalPlace];
    return {
      position: { vertical, horizontal },
      format,
      startNumber: Math.max(0, Math.floor(startNumber) || 0),
      skipFirstPage,
      pages: applyTo === "all" ? null : context.target.pages,
    };
  });
  const numbered = $derived((settings.pages?.length ?? count) - (skipFirstPage ? 1 : 0));
</script>

<ToolSection
  tool="pageNumbers"
  {open}
  {onToggle}
  actionLabel="Add Page Numbers"
  effect={pageNumbersEffect(settings, count)}
  ready={numbered > 0}
  onAction={() => context.change(() => addPageNumbers(context.document.id, settings))}
>
  <label class="setting">
    <span class="caption">Place</span>
    <select class="field" bind:value={place}>
      {#each PLACES as [value, label] (value)}
        <option {value}>{label}</option>
      {/each}
    </select>
  </label>
  <label class="setting">
    <span class="caption">Style</span>
    <select class="field" bind:value={format}>
      {#each FORMATS as [value, label] (value)}
        <option {value}>{label}</option>
      {/each}
    </select>
  </label>
  <label class="setting">
    <span class="caption">Start At</span>
    <input class="field" type="number" min="0" bind:value={startNumber} />
  </label>
  <label class="check">
    <input type="checkbox" bind:checked={skipFirstPage} />
    <span>No number on the first page (a title page)</span>
  </label>
  <PagesChoice {context} bind:value={applyTo} />
</ToolSection>
