<script lang="ts">
  // takes off page numbers, headers and footers, or watermarks that the
  // tools above added. nothing else on the pages is touched.
  import type { ToolContext } from "../../inspector";
  import { removeStamps, type StampKind } from "../../pageApi";
  import { removeEffect } from "../../pageEffects";
  import Choices from "../Choices.svelte";
  import PagesChoice from "../PagesChoice.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let kind = $state<StampKind>("pageNumbers");
  let applyTo = $state<"all" | "target">("all");
  const count = $derived(context.document.pages.length);
  const pages = $derived(applyTo === "all" ? null : context.target.pages);
</script>

<ToolSection
  tool="removeStamps"
  {open}
  {onToggle}
  actionLabel="Remove"
  effect={removeEffect(kind, pages, count)}
  onAction={() => context.change(() => removeStamps(context.document.id, kind, pages))}
>
  <Choices
    label="What to Remove"
    options={[
      { value: "pageNumbers", label: "Numbers" },
      { value: "headerFooter", label: "Headers" },
      { value: "watermark", label: "Watermarks" },
    ]}
    bind:value={kind}
  />
  <p class="hint">Only what Letters added is removed. Headers includes footers.</p>
  <PagesChoice {context} bind:value={applyTo} />
</ToolSection>
