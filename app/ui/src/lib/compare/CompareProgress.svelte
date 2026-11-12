<script lang="ts">
  // while comparing: what it is doing, how far it has got, and Stop.
  import { tooltipFor } from "../tools";
  import type { JobStatus } from "./compareApi";
  import { progressFraction, progressText } from "./progress";

  interface Props {
    status: JobStatus;
    names: { older: string; newer: string };
    onStop: () => void;
  }

  let { status, names, onStop }: Props = $props();

  const percent = $derived(Math.round(progressFraction(status) * 100));
</script>

<div class="progress">
  <h1>Comparing</h1>
  <p class="names">“{names.older}” and “{names.newer}”</p>
  <div
    class="bar"
    role="progressbar"
    aria-label="Comparison"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={percent}
    aria-valuetext={progressText(status)}
  >
    <div class="fill" style:width="{percent}%"></div>
  </div>
  <p class="hint" aria-live="polite">{progressText(status)}</p>
  <div class="actions">
    <button class="button" title={tooltipFor("compareStop")} onclick={onStop}>Stop</button>
  </div>
</div>

<style>
  .progress {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: min(30rem, calc(100% - 2rem));
    margin: 0 auto;
    padding: calc(var(--toolbar-space) + 3rem) 0;
  }

  h1 {
    margin: 0;
    font: var(--title);
  }

  .names {
    margin: 0;
    color: var(--label-secondary);
    overflow-wrap: anywhere;
  }

  .bar {
    height: 0.375rem;
    overflow: hidden;
    border-radius: 0.25rem;
    background: var(--control-active);
  }

  .fill {
    height: 100%;
    border-radius: 0.25rem;
    background: var(--accent);
    transition: width 0.15s var(--ease);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
