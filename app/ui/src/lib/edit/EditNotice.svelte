<script lang="ts">
  // what Letters says beside text it just changed: which font drew letters
  // the original font lacks (never a silent substitution), and, when the
  // new text needs more lines than its paragraph had, the choice between
  // Let It Grow and Shrink to Fit.
  import type { PageFrame } from "../pageTools/pagePoints";
  import type { EditSession } from "./editSession.svelte";

  interface Props {
    session: EditSession;
    frame: PageFrame;
  }

  let { session, frame }: Props = $props();

  const notice = $derived(session.notice!);
  const left = $derived(frame.left + notice.rect.x * frame.scale);
  const top = $derived(frame.top + (notice.rect.y + notice.rect.height) * frame.scale + 6);
</script>

<div class="notice glass" role="status" data-page-ui data-edit-notice style:left="{left}px" style:top="{top}px">
  {#each notice.sentences as sentence (sentence)}
    <p>{sentence}</p>
  {/each}
  <div class="actions">
    {#if notice.overflow}
      <button class="button" onclick={() => session.shrinkToFit()}>Shrink to Fit</button>
      <button class="button primary" onclick={() => session.letItGrow()}>Let It Grow</button>
    {:else}
      <button class="button" onclick={() => (session.notice = null)}>OK</button>
    {/if}
  </div>
</div>

<style>
  .notice {
    position: absolute;
    z-index: 3;
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    width: min(22rem, calc(100vw - 2rem));
    padding: 0.625rem 0.75rem;
  }

  p {
    margin: 0;
    line-height: 1.125rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
