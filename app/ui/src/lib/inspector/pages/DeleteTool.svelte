<script lang="ts">
  // takes the selected pages (or the page being read) out of the document.
  import type { ToolContext } from "../../inspector";
  import { deletePages } from "../../pageApi";
  import { deleteEffect, targetHint } from "../../pageEffects";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const count = $derived(context.document.pages.length);
  const outcome = $derived(deleteEffect(context.target.pages, count));
</script>

<ToolSection
  tool="deletePages"
  {open}
  {onToggle}
  actionLabel="Delete Pages"
  effect={outcome.text}
  ready={!outcome.blocked}
  onAction={() => context.change(() => deletePages(context.document.id, context.target.pages))}
>
  <p class="hint">{targetHint(context.target, count)} The Delete key does the same in the sidebar.</p>
</ToolSection>
