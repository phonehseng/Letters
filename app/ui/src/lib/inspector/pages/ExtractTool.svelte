<script lang="ts">
  // saves copies of some pages as a new pdf, then offers to open it.
  import { untrack } from "svelte";
  import { asCommandError, chooseSaveLocation, newWindow } from "../../api";
  import type { ToolContext } from "../../inspector";
  import { extractPages, fileNameOf, pathBeside, stemOf } from "../../pageApi";
  import { extractEffect } from "../../pageEffects";
  import { describePages, pagesAsTypedText, parsePageRanges } from "../../pageRanges";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let text = $state("");
  // the field follows the selection until the person types in it
  let typed = $state(false);
  $effect(() => {
    const pages = context.target.pages;
    if (!untrack(() => typed)) text = pagesAsTypedText(pages);
  });

  const count = $derived(context.document.pages.length);
  const parsed = $derived(parsePageRanges(text, count));
  const outcome = $derived(parsed.ok ? extractEffect(parsed.pages, count) : parsed.error);

  async function extract() {
    if (!parsed.ok) return;
    const pages = parsed.pages;
    const suggested = pathBeside(context.document.path, `${stemOf(context.document.path)} - ${pagesAsTypedText(pages)}.pdf`);
    const savePath = await chooseSaveLocation(suggested);
    if (!savePath) return;
    try {
      const written = await extractPages(context.document.id, pages, savePath);
      context.notify(`Saved ${describePages(pages, count)} as “${fileNameOf(written)}”.`, {
        label: "Open",
        run: () => newWindow(written),
      });
    } catch (problem) {
      context.showError(asCommandError(problem).message);
    }
  }
</script>

<ToolSection tool="extractPages" {open} {onToggle} actionLabel="Extract…" effect={outcome} ready={parsed.ok} onAction={extract}>
  <label class="setting">
    <span class="caption">Pages</span>
    <input class="field" type="text" spellcheck="false" placeholder="1-3, 7" bind:value={text} oninput={() => (typed = true)} />
  </label>
  <p class="hint">Type page numbers and ranges, such as 1-3, 7. They start as the selected pages.</p>
</ToolSection>
