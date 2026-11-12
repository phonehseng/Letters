<script lang="ts">
  // Remove Recognized Text: takes the invisible layer Letters added off
  // the chosen pages. the pictures stay as they are.
  import type { ToolContext } from "../../inspector";
  import { removeEffect } from "../../ocr/ocrSentences";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.scan!);
  const pages = $derived(session.overview?.withLayer ?? []);
</script>

<ToolSection
  tool="removeRecognizedText"
  {open}
  {onToggle}
  actionLabel="Remove Recognized Text"
  ready={pages.length > 0 && !session.busy}
  effect={removeEffect(pages, context.document.pages.length)}
  onAction={() => session.removeText(pages)}
/>
