<script lang="ts">
  // "which pages": every page, or the ones the tool would act on now
  // (the selected pages, or the page being read when nothing is
  // selected). tools that usually apply to the whole document (page
  // numbers, headers, watermarks) start on every page.
  import type { ToolContext } from "../inspector";
  import { describePages } from "../pageRanges";
  import Choices from "./Choices.svelte";

  interface Props {
    context: ToolContext;
    /** "all" or "target" */
    value: "all" | "target";
  }

  let { context, value = $bindable() }: Props = $props();

  const count = $derived(context.document.pages.length);
  const targetLabel = $derived(context.target.fromSelection ? "Selected Pages" : "This Page");
  const describe = $derived(
    value === "all" ? `All ${count} pages.` : `${capitalise(describePages(context.target.pages, count))}.`,
  );

  function capitalise(text: string) {
    return text.charAt(0).toUpperCase() + text.slice(1);
  }
</script>

<Choices
  label="Apply To"
  options={[
    { value: "all", label: "All Pages" },
    { value: "target", label: targetLabel },
  ]}
  bind:value
/>
<p class="hint">{describe}</p>

<style>
  .hint {
    margin: -0.375rem 0 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }
</style>
