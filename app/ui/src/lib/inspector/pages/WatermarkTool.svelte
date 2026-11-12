<script lang="ts">
  // faint text or a picture across the middle of pages.
  import type { ToolContext } from "../../inspector";
  import { addImageWatermark, addTextWatermark, choosePicture, fileNameOf, type StampLayer } from "../../pageApi";
  import { watermarkEffect } from "../../pageEffects";
  import Choices from "../Choices.svelte";
  import PagesChoice from "../PagesChoice.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let kind = $state<"text" | "picture">("text");
  let text = $state("Draft");
  let picture = $state<string | null>(null);
  let layer = $state<StampLayer>("inFront");
  let opacityPercent = $state(30);
  let applyTo = $state<"all" | "target">("all");

  const count = $derived(context.document.pages.length);
  const pages = $derived(applyTo === "all" ? null : context.target.pages);
  const opacity = $derived(opacityPercent / 100);
  const subject = $derived(
    kind === "text" ? { kind: "text" as const, text } : { kind: "picture" as const, name: picture ? fileNameOf(picture) : null },
  );
  const ready = $derived(kind === "text" ? text.trim() !== "" : picture !== null);

  async function choose() {
    picture = (await choosePicture()) ?? picture;
  }

  function add() {
    const id = context.document.id;
    if (kind === "text") return context.change(() => addTextWatermark(id, { text: text.trim(), opacity, layer, pages }));
    if (!picture) return;
    const path = picture;
    return context.change(() => addImageWatermark(id, path, { opacity, scale: 0.5, layer, pages }));
  }
</script>

<ToolSection
  tool="watermark"
  {open}
  {onToggle}
  actionLabel="Add Watermark"
  effect={watermarkEffect(subject, layer, opacity, pages, count)}
  {ready}
  onAction={add}
>
  <Choices
    label="Watermark"
    hideLabel
    options={[
      { value: "text", label: "Text" },
      { value: "picture", label: "Picture" },
    ]}
    bind:value={kind}
  />
  {#if kind === "text"}
    <label class="setting">
      <span class="caption">Text</span>
      <input class="field" type="text" bind:value={text} />
    </label>
  {:else}
    <div class="setting">
      <span class="caption">Picture</span>
      <button class="button" onclick={choose}>{picture ? "Choose Another Picture…" : "Choose Picture…"}</button>
      {#if picture}<p class="hint">“{fileNameOf(picture)}”, half as wide as the page.</p>{/if}
    </div>
  {/if}
  <Choices
    label="Layer"
    options={[
      { value: "inFront", label: "In Front", title: "Over the page’s text and pictures" },
      { value: "behind", label: "Behind", title: "Under the page’s text and pictures" },
    ]}
    bind:value={layer}
  />
  {#if layer === "behind"}
    <p class="hint">Behind a scanned page, which is one big picture, the watermark can’t be seen.</p>
  {/if}
  <label class="setting">
    <span class="caption">Opacity: {opacityPercent}%</span>
    <input type="range" min="5" max="100" step="5" bind:value={opacityPercent} aria-valuetext={`${opacityPercent}%`} />
  </label>
  <PagesChoice {context} bind:value={applyTo} />
</ToolSection>
