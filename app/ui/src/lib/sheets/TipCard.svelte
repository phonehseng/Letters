<script lang="ts">
  // one tip at the moment it helps (the engine's help::suggest): what it
  // is, why, and what doing it puts in the sheet. Do It is the one accent
  // button; Not Now hides this tip here for good, Don't Show Again every
  // tip of its kind. it never covers the cells being worked on: it sits in
  // the bottom corner, above the status bar.
  import type { Tip } from "./sheetsApi";

  interface Props {
    tip: Tip;
    onApply: () => void;
    onNotNow: () => void;
    onNever: () => void;
  }

  let { tip, onApply, onNotNow, onNever }: Props = $props();
</script>

<div class="glass tip" role="status" aria-label="Tip">
  <p class="kicker">Tip</p>
  <h3>{tip.title}</h3>
  <p>{tip.why}</p>
  {#if tip.preview}<p class="preview">{tip.preview}</p>{/if}
  <div class="buttons">
    <button class="button link" onclick={onNever}>Don’t Show Again</button>
    <button class="button" onclick={onNotNow}>Not Now</button>
    <button class="button primary" onclick={onApply}>Do It</button>
  </div>
</div>

<style>
  .tip {
    position: absolute;
    right: 1rem;
    bottom: 4rem;
    z-index: 4;
    width: min(22rem, calc(100% - 2rem));
    padding: 0.75rem 0.875rem;
    border-radius: var(--radius-group);
    animation: arrive 0.2s var(--ease);
  }

  .kicker {
    margin: 0;
    color: var(--accent-label);
    font: var(--footnote);
    font-weight: 600;
    letter-spacing: var(--footnote-spacing);
    text-transform: uppercase;
  }

  h3 {
    margin: 0.125rem 0 0.25rem;
    font: var(--headline);
  }

  p {
    margin: 0.25rem 0;
    font: var(--body);
  }

  .preview {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .link {
    margin-right: auto;
    background: transparent;
    color: var(--label-secondary);
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(0.5rem);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .tip {
      animation: none;
    }
  }
</style>
