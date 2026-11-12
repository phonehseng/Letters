<script lang="ts">
  // a line of text along the top (header) or bottom (footer) of pages,
  // in up to three places. tokens fill in the page number, the number of
  // pages, today's date and the file name.
  import type { ToolContext } from "../../inspector";
  import { addHeaderFooter, fileNameOf, type HeaderFooterSettings } from "../../pageApi";
  import { headerFooterEffect } from "../../pageEffects";
  import Choices from "../Choices.svelte";
  import PagesChoice from "../PagesChoice.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let edge = $state<"top" | "bottom">("top");
  // the usual student header: the file on the left, the date on the right
  let left = $state("{file}");
  let center = $state("");
  let right = $state("{date}");
  let applyTo = $state<"all" | "target">("all");

  const today = new Date().toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" });
  const count = $derived(context.document.pages.length);
  const settings = $derived<HeaderFooterSettings>({
    edge,
    left,
    center,
    right,
    pages: applyTo === "all" ? null : context.target.pages,
    fileName: fileNameOf(context.document.path),
    date: today,
  });
  const ready = $derived([left, center, right].some((text) => text.trim() !== ""));
</script>

<ToolSection
  tool="headerFooter"
  {open}
  {onToggle}
  actionLabel={edge === "top" ? "Add Header" : "Add Footer"}
  effect={headerFooterEffect(settings, count)}
  {ready}
  onAction={() => context.change(() => addHeaderFooter(context.document.id, settings))}
>
  <Choices
    label="Where"
    hideLabel
    options={[
      { value: "top", label: "Header" },
      { value: "bottom", label: "Footer" },
    ]}
    bind:value={edge}
  />
  <label class="setting">
    <span class="caption">Left</span>
    <input class="field" type="text" bind:value={left} />
  </label>
  <label class="setting">
    <span class="caption">Centre</span>
    <input class="field" type="text" bind:value={center} />
  </label>
  <label class="setting">
    <span class="caption">Right</span>
    <input class="field" type="text" bind:value={right} />
  </label>
  <p class="hint">{"{page}"} becomes the page number, {"{pages}"} the number of pages, {"{date}"} today’s date and {"{file}"} the file name.</p>
  <PagesChoice {context} bind:value={applyTo} />
</ToolSection>
