<script lang="ts">
  // the line at the top of the Protect tab: how the file is locked, and
  // when a new password or restriction takes effect (only when saving).
  // a second line says when saving will write the file from scratch.
  import type { ProtectionStatus } from "../../protect/protectApi";
  import { cleanSaveLine, lockLine } from "../../protect/protectEffects";

  interface Props {
    status: ProtectionStatus | null;
  }

  let { status }: Props = $props();

  const lock = $derived(lockLine(status));
  const clean = $derived(cleanSaveLine(status));
</script>

{#if lock || clean}
  <div class="status" role="status">
    {#if lock}
      <p class="lock">
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
          <path d="M7 10.5V8a5 5 0 0 1 10 0v2.5 M6 10.5h12a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1Z" />
        </svg>
        <span>{lock}</span>
      </p>
    {/if}
    {#if clean}
      <p class="hint">{clean}</p>
    {/if}
  </div>
{/if}

<style>
  .status {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0 0 0.5rem;
    padding: 0.5rem 0.625rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  p {
    margin: 0;
  }

  .lock {
    display: flex;
    align-items: flex-start;
    gap: 0.375rem;
    line-height: 1.125rem;
  }

  svg {
    flex-shrink: 0;
    margin-top: 0.0625rem;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
