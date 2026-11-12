<script lang="ts">
  // Show Recognized Text: the words the computer read, over the scan, so
  // a student can check them. doubtful ones are tinted; clicking a word
  // corrects it in the invisible text.
  import type { ToolContext } from "../../inspector";
  import { pagesInWords } from "../../ocr/pageRange";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.scan!);
  const recognized = $derived(session.overview?.recognized ?? []);
</script>

<ToolSection
  tool="showRecognizedText"
  {open}
  {onToggle}
  actionLabel={session.showText ? "Hide Recognized Text" : "Show Recognized Text"}
  pressed={session.showText}
  ready={recognized.length > 0}
  effect={recognized.length === 0
    ? "No page has recognized text yet. Use Recognize Text first."
    : `Shows the words read on ${pagesInWords(recognized, context.document.pages.length)}. Nothing changes until you correct a word.`}
  onAction={() => session.toggleText()}
>
  <p class="hint">Words Letters was less than 70% sure of are tinted yellow. Click a word, type the right one and press Enter; Escape leaves it.</p>
</ToolSection>
