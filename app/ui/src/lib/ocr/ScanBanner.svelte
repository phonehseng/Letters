<script lang="ts">
  // the quiet banner on a document with scanned pages nobody has read yet:
  // one sentence and Make This Document Searchable, which opens the Scan
  // tab with those pages chosen. it never covers the page for long: Not
  // Now closes it for this document.
  import { bannerSentence } from "./ocrSentences";
  import type { ScanSession } from "./scanSession.svelte";

  interface Props {
    session: ScanSession;
    top: number;
    onOpen: () => void;
  }

  let { session, top, onOpen }: Props = $props();

  const pageCount = $derived(session.overview?.pageCount ?? 0);
</script>

{#if session.showBanner}
  <div class="banner glass" role="region" aria-label="Scanned document" style:top="{top + 8}px">
    <span>{bannerSentence(session.unreadScans.length, pageCount)}</span>
    <button class="button" onclick={() => (session.bannerClosed = true)}>Not Now</button>
    <button
      class="button primary"
      onclick={() => {
        session.pageChoice = session.unreadScans.length < pageCount ? "scanned" : "all";
        onOpen();
      }}
    >
      Make This Document Searchable
    </button>
  </div>
{/if}

<style>
  .banner {
    position: absolute;
    left: 50%;
    z-index: 2;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.625rem;
    max-width: min(36rem, calc(100% - 2rem));
    padding: 0.5rem 0.5rem 0.5rem 1rem;
    transform: translateX(-50%);
  }

  .banner span {
    flex: 1 1 14rem;
  }
</style>
