<script lang="ts">
  // text too pale for its background: the measured ratio and what it
  // needs, both colours, and the nearest colour that passes (same hue,
  // darker or lighter). applying recolours those pieces of text.
  import type { ContentRef, Finding, Rgb } from "../../../a11y/a11yApi";
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import { contrastRatio, hex, ratioText } from "../../../a11y/contrast";

  interface Props {
    session: A11ySession;
    finding: Finding;
    runs: ContentRef[];
    suggested: Rgb;
  }

  let { session, finding, runs, suggested }: Props = $props();

  let busy = $state(false);
  const measured = $derived(finding.contrast);
  const after = $derived(measured ? contrastRatio({ ...suggested, a: 1 }, { ...measured.backgroundColour, a: 1 }) : null);

  async function apply() {
    busy = true;
    await session.fix({ kind: "changeTextColour", runs, colour: suggested });
    busy = false;
  }
</script>

{#if measured}
  <dl class="colours">
    <div>
      <dt>Text now</dt>
      <dd><span class="swatch" style:background={hex(measured.textColour)}></span>{hex(measured.textColour)}</dd>
    </div>
    <div>
      <dt>Background</dt>
      <dd><span class="swatch" style:background={hex(measured.backgroundColour)}></span>{hex(measured.backgroundColour)}</dd>
    </div>
    <div>
      <dt>Suggested</dt>
      <dd><span class="swatch" style:background={hex(suggested)}></span>{hex(suggested)}</dd>
    </div>
  </dl>
  <p>
    Contrast is {ratioText(measured.ratio)}; this text needs {ratioText(measured.required)}.
    {#if after}The suggested colour gives {ratioText(after)}.{/if}
  </p>
  <p class="sample" style:color={hex(suggested)} style:background={hex(measured.backgroundColour)}>Sample text in the suggested colour</p>
{/if}
<div class="actions">
  <button class="button primary" disabled={busy || runs.length === 0} onclick={apply}>Use Suggested Colour</button>
</div>
<p class="hint">
  {runs.length === 0
    ? "This text is inside a part of the page Letters can’t recolour. Change its colour where the document was made."
    : "Recolours this text in your working copy. Undo with Ctrl Z."}
</p>

<style>
  .colours {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.375rem;
    margin: 0;
  }

  dt {
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  dd {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    margin: 0;
    font: var(--subheadline);
  }

  .swatch {
    width: 0.875rem;
    height: 0.875rem;
    border-radius: 0.25rem;
    box-shadow: inset 0 0 0 0.5px var(--separator);
  }

  p {
    margin: 0;
    line-height: 1.125rem;
  }

  .sample {
    padding: 0.375rem 0.5rem;
    border-radius: var(--radius-small);
    box-shadow: inset 0 0 0 0.5px var(--separator);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
