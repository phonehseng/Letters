<script lang="ts">
  // a part of a page, drawn by the pdf engine, so the person can see the
  // picture or table a step is about without hunting for it.
  import { regionPicture, type Rect } from "../../../a11y/a11yApi";

  interface Props {
    documentId: number;
    revision: number;
    page: number;
    rect: Rect;
    /** what it shows, for screen readers */
    label: string;
  }

  let { documentId, revision, page, rect, label }: Props = $props();

  let source = $state<string | null>(null);
  let failed = $state(false);

  $effect(() => {
    void revision;
    let current = true;
    regionPicture(documentId, page, rect)
      .then((url) => current && (source = url))
      .catch(() => current && (failed = true));
    return () => {
      current = false;
    };
  });
</script>

<div class="picture">
  {#if source}
    <img src={source} alt={label} />
  {:else if failed}
    <p class="hint">This part of the page can’t be shown.</p>
  {:else}
    <p class="hint">Drawing…</p>
  {/if}
</div>

<style>
  .picture {
    display: grid;
    place-items: center;
    min-height: 3rem;
    max-height: 12rem;
    padding: 0.375rem;
    overflow: hidden;
    border-radius: var(--radius-small);
    background: #ffffff;
    box-shadow: inset 0 0 0 0.5px var(--separator);
  }

  img {
    max-width: 100%;
    max-height: 11rem;
    object-fit: contain;
  }
</style>
