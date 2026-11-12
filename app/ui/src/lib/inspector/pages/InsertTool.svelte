<script lang="ts">
  // adds blank pages, or pages copied from another pdf, before or after
  // the pages the tool works on, or at the end.
  import type { ToolContext } from "../../inspector";
  import { chooseFile } from "../../api";
  import { fileNameOf, insertBlankPages, insertPagesFrom, pdfPageCount } from "../../pageApi";
  import { insertBlankEffect, insertFromEffect } from "../../pageEffects";
  import { pagesAsTypedText, parsePageRanges } from "../../pageRanges";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";
  import { asCommandError } from "../../api";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let source = $state<"blank" | "pdf">("blank");
  let place = $state<"before" | "after" | "end">("after");
  let blankCount = $state(1);
  let other = $state<{ path: string; name: string; pageCount: number } | null>(null);
  let otherPagesText = $state("");
  let otherProblem = $state<string | null>(null);

  const count = $derived(context.document.pages.length);
  const first = $derived(Math.min(...context.target.pages));
  const last = $derived(Math.max(...context.target.pages));
  const atIndex = $derived(place === "before" ? first : place === "after" ? last + 1 : count);
  // a blank page is the size of the page next to it, so it fits in
  const size = $derived(context.document.pages[Math.min(last, count - 1)] ?? { width: 612, height: 792 });

  const otherPages = $derived(other && otherPagesText.trim() ? parsePageRanges(otherPagesText, other.pageCount) : null);
  const chosenOther = $derived(otherPages && otherPages.ok ? otherPages.pages : null);

  const outcome = $derived.by(() => {
    if (source === "blank") return insertBlankEffect(blankCount, atIndex, count);
    if (otherProblem) return otherProblem;
    if (otherPages && !otherPages.ok) return otherPages.error;
    return insertFromEffect(other, chosenOther?.length ?? null, atIndex, count);
  });
  const ready = $derived(
    source === "blank" ? blankCount >= 1 && blankCount <= 1000 : other !== null && !otherProblem && (otherPages?.ok ?? true),
  );

  async function choosePdf() {
    const path = await chooseFile();
    if (!path) return;
    otherProblem = null;
    try {
      const pageCount = await pdfPageCount(path);
      other = { path, name: fileNameOf(path), pageCount };
      otherPagesText = pagesAsTypedText(Array.from({ length: pageCount }, (_, page) => page));
    } catch (problem) {
      other = null;
      otherProblem = asCommandError(problem).message;
    }
  }

  function insert() {
    const id = context.document.id;
    if (source === "blank") {
      return context.change(() =>
        insertBlankPages(id, { atIndex, widthPt: size.width, heightPt: size.height, count: blankCount }),
      );
    }
    if (!other) return;
    const path = other.path;
    return context.change(() => insertPagesFrom(id, atIndex, path, chosenOther));
  }

  const where = $derived(context.target.pages.length === 1 ? `Page ${first + 1}` : "the Selection");
</script>

<ToolSection tool="insertPages" {open} {onToggle} actionLabel="Insert" effect={outcome} {ready} onAction={insert}>
  <Choices
    label="Add"
    options={[
      { value: "blank", label: "Blank Page" },
      { value: "pdf", label: "From a PDF" },
    ]}
    bind:value={source}
  />
  <Choices
    label="Where"
    options={[
      { value: "before", label: "Before", title: `Before ${where}` },
      { value: "after", label: "After", title: `After ${where}` },
      { value: "end", label: "At the End" },
    ]}
    bind:value={place}
  />
  {#if source === "blank"}
    <label class="setting">
      <span class="caption">How Many</span>
      <input class="field" type="number" min="1" max="1000" bind:value={blankCount} />
    </label>
    <p class="hint">Blank pages are the same size as page {Math.min(last, count - 1) + 1}.</p>
  {:else}
    <div class="setting">
      <span class="caption">PDF</span>
      <button class="button" onclick={choosePdf}>{other ? "Choose Another PDF…" : "Choose PDF…"}</button>
      {#if other}
        <p class="hint">“{other.name}” has {other.pageCount} {other.pageCount === 1 ? "page" : "pages"}.</p>
      {/if}
    </div>
    {#if other}
      <label class="setting">
        <span class="caption">Its Pages</span>
        <input class="field" type="text" spellcheck="false" placeholder="1-3, 7" bind:value={otherPagesText} />
      </label>
    {/if}
  {/if}
</ToolSection>
