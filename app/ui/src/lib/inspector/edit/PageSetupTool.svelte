<script lang="ts">
  // Page Setup: the margins, paper size, extra space and page boxes of the
  // page being read, in a sheet (edit/PageSetupSheet.svelte) since they are
  // many numbers. the section says what each part does before it opens.
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.edit!);
</script>

<ToolSection
  tool="pageSetup"
  {open}
  {onToggle}
  actionLabel="Page Setup…"
  effect={`Opens Page Setup for page ${context.currentPage + 1}. Nothing changes until you choose Apply there.`}
  onAction={() => session.pageSetup.show(context.currentPage)}
>
  <ul class="hint">
    <li>Margins move the content so it sits the distance you choose from each edge.</li>
    <li>Paper Size puts the page on a new sheet, the content fitted to it.</li>
    <li>Extra Space adds white space on one side without moving anything.</li>
    <li>Page Boxes are the exact areas a PDF uses for the paper, what shows, and printing.</li>
  </ul>
</ToolSection>

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding-left: 1rem;
  }
</style>
