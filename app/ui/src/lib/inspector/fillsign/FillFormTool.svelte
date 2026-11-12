<script lang="ts">
  // Fill Form: the progress line, Show Fields, the list of fields, and
  // Next Empty Field. the filling itself happens on the page. a form the
  // engine can't fill (xfa) says why. totals, formats and checks run as
  // each answer is filled; a form with scripts Letters leaves alone says
  // which fields, in the engine's own sentence.
  import { fillEffect } from "../../forms/fillSignEffects";
  import { formProgress, isFilled, isFillable } from "../../forms/tabOrder";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";
  import FieldList from "./FieldList.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.fillSign!);
  const progress = $derived(formProgress(session.fields));
  const empty = $derived(session.fields.filter((field) => isFillable(field) && !isFilled(field)).length);
</script>

<ToolSection
  tool="fillForm"
  {open}
  {onToggle}
  actionLabel="Next Empty Field"
  ready={session.stops.length > 0}
  effect={fillEffect(session.stops.length, empty)}
  onAction={() => {
    session.nextEmpty();
  }}
>
  {#if session.overview?.problem}
    <p class="note" role="alert">{session.overview.problem}</p>
  {/if}
  {#if session.overview?.customScriptNote}
    <p class="note">{session.overview.customScriptNote}. Fields with the usual totals, formats and checks still work.</p>
  {/if}
  {#if session.fields.length > 0}
    <div class="row">
      <label class="check"><input type="checkbox" bind:checked={session.showFields} /> Show Fields</label>
      <span class="progress" aria-live="polite">{progress.text}</span>
    </div>
    <FieldList {session} />
    <p class="hint">Click a field on the page and type. Tab and Shift Tab go to the next and previous field; Space ticks a checkbox.</p>
  {/if}
</ToolSection>

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .progress {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .note {
    margin: 0;
    padding: 0.375rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
    font: var(--subheadline);
    line-height: 0.9375rem;
    letter-spacing: var(--subheadline-spacing);
  }
</style>
