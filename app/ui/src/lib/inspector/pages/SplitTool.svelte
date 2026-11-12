<script lang="ts">
  // cuts the document into several new pdfs in a folder the person picks.
  // four ways to cut, each explained in a sentence.
  import { asCommandError, chooseFolder, newWindow } from "../../api";
  import type { ToolContext } from "../../inspector";
  import { fileNameOf, splitDocument, stemOf, type SplitSettings } from "../../pageApi";
  import { splitEffect } from "../../pageEffects";
  import { parsePageRanges } from "../../pageRanges";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const count = $derived(context.document.pages.length);
  let mode = $state<SplitSettings["mode"]>("everyNPages");
  // a sensible first guess: cut in half
  let rangesText = $state("");
  $effect(() => {
    if (rangesText === "") rangesText = halves(count);
  });
  let pagesPerPart = $state(10);
  let megabytesLimit = $state(10);

  const EXPLANATIONS: Record<SplitSettings["mode"], string> = {
    byRanges: "One new PDF for each range you type. Ranges can overlap.",
    everyNPages: "A new PDF every so many pages. The last one may be shorter.",
    byBookmarks: "One new PDF for each chapter in the document’s outline (its top-level bookmarks).",
    byMaxSize: "As few PDFs as possible, each small enough to upload under the size you set.",
  };

  function halves(pageCount: number): string {
    if (pageCount < 2) return "1";
    const half = Math.ceil(pageCount / 2);
    return `1-${half}, ${half + 1}-${pageCount}`;
  }

  const ranges = $derived(parsePageRanges(rangesText, count));
  const settings = $derived.by((): SplitSettings | null => {
    if (mode === "byRanges") return ranges.ok ? { mode, ranges: ranges.spans } : null;
    if (mode === "everyNPages") return pagesPerPart >= 1 ? { mode, pagesPerPart: Math.floor(pagesPerPart) } : null;
    if (mode === "byMaxSize") return megabytesLimit > 0 ? { mode, maxBytes: Math.round(megabytesLimit * 1024 * 1024) } : null;
    return { mode: "byBookmarks" };
  });
  const outcome = $derived.by(() => {
    if (settings) return splitEffect(settings, count);
    if (mode === "byRanges" && !ranges.ok) return ranges.error;
    return mode === "everyNPages" ? "Choose at least 1 page for each part." : "Choose a size bigger than zero.";
  });

  async function split() {
    if (!settings) return;
    const folder = await chooseFolder();
    if (!folder) return;
    try {
      const files = await splitDocument(context.document.id, settings, folder, stemOf(context.document.path));
      const names = files.map((file) => `“${file.name}”`).join(", ");
      const tooBig = files.some((file) => file.overSizeLimit)
        ? " Some single pages are bigger than the size you chose, so they got a file of their own."
        : "";
      context.notify(`Made ${files.length} ${files.length === 1 ? "PDF" : "PDFs"} in “${fileNameOf(folder)}”: ${names}.${tooBig}`, {
        label: "Open First",
        run: () => newWindow(files[0].path),
      });
    } catch (problem) {
      context.showError(asCommandError(problem).message);
    }
  }
</script>

<ToolSection tool="splitDocument" {open} {onToggle} actionLabel="Split…" effect={outcome} ready={settings !== null} onAction={split}>
  <Choices
    label="Split By"
    options={[
      { value: "byRanges", label: "Ranges" },
      { value: "everyNPages", label: "Every N" },
      { value: "byBookmarks", label: "Chapters" },
      { value: "byMaxSize", label: "Size" },
    ]}
    bind:value={mode}
  />
  <p class="hint">{EXPLANATIONS[mode]}</p>
  {#if mode === "byRanges"}
    <label class="setting">
      <span class="caption">Ranges</span>
      <input class="field" type="text" spellcheck="false" placeholder="1-3, 4-" bind:value={rangesText} />
    </label>
  {:else if mode === "everyNPages"}
    <label class="setting">
      <span class="caption">Pages in Each PDF</span>
      <input class="field" type="number" min="1" bind:value={pagesPerPart} />
    </label>
  {:else if mode === "byMaxSize"}
    <label class="setting">
      <span class="caption">Largest Size, in MB</span>
      <input class="field" type="number" min="0.1" step="0.5" bind:value={megabytesLimit} />
    </label>
  {/if}
</ToolSection>
