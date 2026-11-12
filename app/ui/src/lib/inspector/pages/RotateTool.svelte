<script lang="ts">
  // turns the selected pages (or the page being read).
  import type { ToolContext } from "../../inspector";
  import { rotatePages } from "../../pageApi";
  import { rotateEffect, targetHint } from "../../pageEffects";
  import Choices from "../Choices.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  let turns = $state<"-1" | "1" | "2">("1");
  const count = $derived(context.document.pages.length);
</script>

<ToolSection
  tool="rotatePages"
  {open}
  {onToggle}
  actionLabel="Rotate"
  effect={rotateEffect(Number(turns), context.target.pages, count)}
  onAction={() => context.change(() => rotatePages(context.document.id, context.target.pages, Number(turns)))}
>
  <Choices
    label="Turn"
    options={[
      { value: "-1", label: "Left" },
      { value: "1", label: "Right" },
      { value: "2", label: "Upside Down" },
    ]}
    bind:value={turns}
  />
  <p class="hint">{targetHint(context.target, count)}</p>
</ToolSection>
