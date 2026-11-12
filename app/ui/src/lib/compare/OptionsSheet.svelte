<script lang="ts">
  // Options… after a comparison: what counts as a change, and Compare Again.
  import ProtectSheet from "../protect/ProtectSheet.svelte";
  import type { CompareOptions } from "./compareApi";
  import CompareOptionsForm from "./CompareOptionsForm.svelte";

  interface Props {
    options: CompareOptions;
    onCompare: (options: CompareOptions) => void;
    onCancel: () => void;
  }

  let { options, onCompare, onCancel }: Props = $props();

  // a copy, so Cancel leaves the options as they were
  // svelte-ignore state_referenced_locally
  let draft = $state({ ...options });
</script>

<ProtectSheet id="compare-options" role="dialog" title="Compare Options" message="Choose what counts as a change, then compare again." {onCancel}>
  <CompareOptionsForm options={draft} onChange={(next) => (draft = next)} />
  {#snippet buttons()}
    <button class="button" onclick={onCancel}>Cancel</button>
    <button class="button primary" data-autofocus onclick={() => onCompare(draft)}>Compare Again</button>
  {/snippet}
</ProtectSheet>
