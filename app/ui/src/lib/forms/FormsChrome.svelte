<script lang="ts">
  // the Fill & Sign parts that belong to the whole window rather than a
  // page: a quiet banner when the document is signed (or its form can't
  // be filled), and the question asked before the first change to a
  // signed document.
  //
  // the banner sits at the bottom of the window, so it never covers the
  // messages that appear under the toolbar.
  import type { FillSignSession } from "./fillSignSession.svelte";
  import SignedEditSheet from "./SignedEditSheet.svelte";
  import { bannerText } from "./signatures";

  interface Props {
    session: FillSignSession;
    onShowSignatures: () => void;
  }

  let { session, onShowSignatures }: Props = $props();

  const problem = $derived(session.overview?.problem ?? null);
  const banner = $derived(problem ?? bannerText(session.signatures));
  const showBanner = $derived(banner !== "" && !session.bannerDismissed);
</script>

{#if showBanner}
  <div class="glass banner" role="status">
    <svg class="seal" viewBox="0 0 16 16" aria-hidden="true">
      {#if problem}<path d="M8 2.5 14 13H2Z M8 6.5v3 M8 11.2v.1" />{:else}<path d="M3 8.5 6.5 12 13 4.5" />{/if}
    </svg>
    <span>{banner}</span>
    {#if !problem}<button class="button" onclick={onShowSignatures}>Show Signatures</button>{/if}
    <button class="button" onclick={() => (session.bannerDismissed = true)}>Hide</button>
  </div>
{/if}

{#if session.question}
  <SignedEditSheet onChoose={(choice) => session.question?.answer(choice)} />
{/if}

<style>
  /* quieter than a message: smaller type, and Hide puts it away */
  .banner {
    position: absolute;
    bottom: 1rem;
    left: 50%;
    z-index: 2;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    max-width: min(34rem, calc(100% - 2rem));
    padding: 0.375rem 0.375rem 0.375rem 0.75rem;
    transform: translateX(-50%);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .banner span {
    flex: 1 1 12rem;
  }

  .seal {
    width: 0.875rem;
    height: 0.875rem;
    fill: none;
    stroke: var(--label-secondary);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .banner .button {
    padding: 0.1875rem 0.625rem;
    font: var(--subheadline);
  }
</style>
