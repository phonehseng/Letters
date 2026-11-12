<script lang="ts">
  // Delete Text: takes the paragraph being edited out of the page. a few
  // words are removed by selecting them in the editor and pressing Delete,
  // as in any text field.
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.edit!);
  const draft = $derived(session.editor?.block ? session.editor : null);
</script>

<ToolSection
  tool="deletePageText"
  {open}
  {onToggle}
  actionLabel="Delete Paragraph"
  ready={draft !== null}
  effect={draft
    ? `Takes this paragraph out of page ${draft.page + 1}; its space stays empty. Undo with Ctrl Z.`
    : "Click into a paragraph on the page with Edit Text first."}
  onAction={() => session.deleteParagraph()}
>
  <p class="hint">To remove a few words, select them while editing and press Delete.</p>
</ToolSection>
