<script lang="ts">
  // asked before closing a document whose marks for redaction were never
  // applied: they live only in this window, so closing loses them. Cancel
  // keeps the document open and has the focus.
  import { discardMarksQuestion } from "./protectGuard";
  import ProtectSheet from "./ProtectSheet.svelte";

  interface Props {
    count: number;
    onAnswer: (discard: boolean) => void;
  }

  let { count, onAnswer }: Props = $props();

  const question = $derived(discardMarksQuestion(count));
</script>

<ProtectSheet id="discard-marks" title={question.title} message={question.message} onCancel={() => onAnswer(false)}>
  {#snippet buttons()}
    <button class="button" onclick={() => onAnswer(true)}>Discard Marks</button>
    <span class="space"></span>
    <button class="button primary" data-autofocus onclick={() => onAnswer(false)}>Cancel</button>
  {/snippet}
</ProtectSheet>
