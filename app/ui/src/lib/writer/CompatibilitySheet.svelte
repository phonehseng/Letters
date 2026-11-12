<script lang="ts">
  // Strict Word mode's "Show" list: each thing Word would show differently,
  // in the engine's own sentence (compat::word_compatibility), with Go To
  // and, where the engine has one, a Fix. Fix All applies every fix in one
  // undo step.
  import { fixableCount, fixLabel } from "./strictMode";
  import { blockPos } from "./textBlocks";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    top: number;
    onClose: () => void;
  }

  let { session, top, onClose }: Props = $props();
  const issues = $derived(session.issues);
  const fixable = $derived(fixableCount(issues));

  function goTo(block: number) {
    const doc = session.editor.doc;
    if (block >= doc.childCount) return;
    session.goTo(blockPos(doc, block) + 1);
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="glass sheet" role="dialog" aria-label="Word compatibility" style:top="{top + 8}px">
  <h2>{issues.length ? `${issues.length} thing${issues.length === 1 ? "" : "s"} will look different in Word` : "Everything here opens the same in Word."}</h2>
  {#if issues.length}
    <ul>
      {#each issues as issue, index (index)}
        <li>
          <span class="message">{issue.message}</span>
          <span class="actions">
            {#if issue.location}
              <button class="button" onclick={() => goTo(issue.location!.block)}>Go To</button>
            {/if}
            {#if fixLabel(issue)}
              <button class="button" title="Change only this one. Undo takes it back." onclick={() => session.fixIssues(index)}>{fixLabel(issue)}</button>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="buttons">
    <span class="spacer"></span>
    <button class="button" onclick={onClose}>Done</button>
    {#if fixable}
      <button class="button primary" onclick={() => session.fixIssues()}>Fix All ({fixable})</button>
    {/if}
  </div>
</div>

<style>
  .sheet {
    position: absolute;
    left: 50%;
    z-index: 4;
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: min(32rem, calc(100% - 2rem));
    max-height: 60vh;
    padding: 0.875rem 1rem;
    transform: translateX(-50%);
  }

  h2 {
    margin: 0;
    font: var(--headline);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }

  li {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
    justify-content: space-between;
  }

  .message {
    font: var(--body);
  }

  .actions {
    display: flex;
    flex-shrink: 0;
    gap: 0.375rem;
  }

  .buttons {
    display: flex;
    gap: 0.5rem;
  }

  .spacer {
    flex: 1;
  }
</style>
