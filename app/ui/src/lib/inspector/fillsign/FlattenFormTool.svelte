<script lang="ts">
  // Flatten Form: painting the answers into the pages for good. the
  // explanation is the engine's own idea in one sentence; the warning
  // says the answers can't be changed afterwards; signature fields are
  // kept unless the person says otherwise.
  import { flattenEffect } from "../../forms/fillSignEffects";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  let keepSignatures = $state(true);
  const signatureFields = $derived(session.fields.filter((field) => field.kind === "signature").length);
  const count = $derived(session.fields.length - (keepSignatures ? signatureFields : 0));

  async function flatten() {
    const flattened = await session.flatten(!keepSignatures);
    if (flattened > 0) context.notify(`The answers of ${flattened} ${flattened === 1 ? "field are" : "fields are"} now part of the pages.`);
  }
</script>

<ToolSection
  tool="flattenForm"
  {open}
  {onToggle}
  actionLabel="Flatten"
  ready={count > 0}
  effect={flattenEffect(count, keepSignatures, signatureFields)}
  onAction={flatten}
>
  <p class="hint warning">Answers can no longer be changed once the form is flattened.</p>
  {#if signatureFields > 0}
    <label class="check"><input type="checkbox" bind:checked={keepSignatures} /> Keep signature fields</label>
  {/if}
</ToolSection>

<style>
  .warning {
    color: var(--label);
  }
</style>
