<script lang="ts">
  // every comment in the document, with its replies, in reading order.
  // clicking one goes to it on the page and selects it. each can be
  // answered, edited or deleted here.
  import type { AnnotationInfo } from "../../annotate/annotateApi";
  import { commentByline, commentsSummary, commentText, commentThreads } from "../../annotate/comments";
  import type { ToolContext } from "../../inspector";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    open: boolean;
    onToggle: () => void;
  }

  let { context, open, onToggle }: Props = $props();

  const session = $derived(context.annotate);
  const threads = $derived(commentThreads(session.annotations));
  // "5 minutes ago" is worked out again after every change to the document
  const now = $derived.by(() => {
    void context.document.revision;
    return Date.now() / 1000;
  });

  // the one comment being answered or edited, and the words typed so far
  let replying = $state<string | null>(null);
  let editing = $state<string | null>(null);
  let words = $state("");

  function startReply(root: AnnotationInfo) {
    editing = null;
    replying = root.id;
    words = "";
  }

  function startEdit(annotation: AnnotationInfo) {
    replying = null;
    editing = annotation.id;
    words = annotation.contents ?? "";
  }

  async function send(root: AnnotationInfo) {
    if (words.trim() === "") return;
    if (await session.reply(root, words.trim())) replying = null;
  }

  async function saveEdit(annotation: AnnotationInfo) {
    if (await session.editComment(annotation, words)) editing = null;
  }
</script>

{#snippet item(annotation: AnnotationInfo, root: AnnotationInfo | null)}
  <div class="comment" class:reply={root !== null}>
    <button class="jump" title="Go to it on page {annotation.page + 1}" onclick={() => session.reveal(root ?? annotation)}>
      <span class="byline">{commentByline(annotation, now, root === null)}</span>
      {#if editing !== annotation.id}
        <span class="words">{commentText(annotation)}</span>
      {/if}
    </button>
    {#if editing === annotation.id}
      <label class="visually-hidden" for="edit-{annotation.id}">Comment</label>
      <textarea id="edit-{annotation.id}" class="field" rows="2" bind:value={words}></textarea>
      <div class="row">
        <button class="button" onclick={() => (editing = null)}>Cancel</button>
        <button class="button" onclick={() => saveEdit(annotation)}>Save</button>
      </div>
    {:else}
      <div class="row">
        {#if root === null}
          <button class="link" onclick={() => startReply(annotation)}>Reply</button>
        {/if}
        <button class="link" onclick={() => startEdit(annotation)}>Edit</button>
        <button class="link" onclick={() => session.deleteById(annotation.page, annotation.id)}>Delete</button>
      </div>
    {/if}
  </div>
{/snippet}

<ToolSection tool="comments" {open} {onToggle}>
  <p class="caption" aria-live="polite">{commentsSummary(threads)}</p>
  {#if threads.length === 0}
    <p class="hint">Add a note, or write a remark on a highlight, and it shows here.</p>
  {/if}
  <ol class="threads">
    {#each threads as thread (thread.root.id)}
      <li>
        {@render item(thread.root, null)}
        {#each thread.replies as answer (answer.id)}
          {@render item(answer, thread.root)}
        {/each}
        {#if replying === thread.root.id}
          <label class="visually-hidden" for="reply-{thread.root.id}">Reply</label>
          <textarea id="reply-{thread.root.id}" class="field" rows="2" placeholder="Reply" bind:value={words}></textarea>
          <div class="row">
            <button class="button" onclick={() => (replying = null)}>Cancel</button>
            <button class="button" disabled={words.trim() === ""} onclick={() => send(thread.root)}>Reply</button>
          </div>
        {/if}
      </li>
    {/each}
  </ol>
</ToolSection>

<style>
  .threads {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    padding-bottom: 0.5rem;
    border-bottom: 0.5px solid var(--separator);
  }

  .comment {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .comment.reply {
    padding-left: 0.75rem;
    border-left: 2px solid var(--separator);
  }

  .jump {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    padding: 0.25rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    text-align: left;
  }

  .jump:hover {
    background: var(--control);
  }

  .byline {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .words {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.375rem;
  }

  /* quiet text buttons, so a long list isn't a wall of grey */
  .link {
    padding: 0.125rem 0.25rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--accent-label);
    font: var(--subheadline);
    font-weight: 500;
    letter-spacing: var(--subheadline-spacing);
  }

  .link:hover {
    background: var(--control);
  }

  textarea {
    width: 100%;
    resize: vertical;
  }
</style>
