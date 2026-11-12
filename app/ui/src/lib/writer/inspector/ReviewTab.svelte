<script lang="ts">
  // the Review tab: Track Changes on or off, every tracked change with
  // Accept and Reject, and the comments with their words. clicking an
  // entry goes to its place in the document.
  import { tools, tooltipFor } from "../../tools";
  import * as edit from "../editing";
  import type { WriterSession } from "../writerSession.svelte";

  interface Props {
    session: WriterSession;
    /** a comment to show first, after clicking its text */
    focusComment?: string | null;
  }

  let { session, focusComment = null }: Props = $props();

  const changes = $derived(session.changes());
  const comments = $derived(session.meta.comments ?? []);
  let newComment = $state("");

  function commentText(blocks: { content?: { text?: string }[] }[]): string {
    return blocks.map((block) => (block.content ?? []).map((inline) => inline.text ?? "").join("")).join("\n");
  }

  /** where a comment's text starts, to go there. */
  function commentPosition(id: string): { from: number; to: number } | null {
    let found: { from: number; to: number } | null = null;
    session.editor.doc.descendants((node, pos) => {
      if (node.marks.some((mark) => mark.type.name === "comment" && (mark.attrs.data as { id?: string }).id === id)) {
        found = found ? { from: found.from, to: pos + node.nodeSize } : { from: pos, to: pos + node.nodeSize };
      }
      return true;
    });
    return found;
  }

  function date(value?: string): string {
    if (!value) return "";
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? "" : parsed.toLocaleDateString(undefined, { day: "numeric", month: "short" });
  }

  const hasSelection = $derived(!session.editor.selection.empty);
</script>

<section aria-labelledby="w-track">
  <h3 id="w-track">{tools.wTrackChanges.label}</h3>
  <label class="row" title={tooltipFor("wTrackChanges")}>
    <span class="hint">{tools.wTrackChanges.meaning}</span>
    <input type="checkbox" role="switch" checked={session.tracking} onchange={(event) => session.setTracking(event.currentTarget.checked)} />
  </label>
  {#if changes.length === 0}
    <p class="hint">No tracked changes.</p>
  {:else}
    <div class="buttons">
      <button class="button" title="Keep every change" onclick={() => session.resolveAllChanges(true)}>Accept All</button>
      <button class="button" title="Undo every change" onclick={() => session.resolveAllChanges(false)}>Reject All</button>
    </div>
    <ul class="list" aria-label="Tracked changes">
      {#each changes as change (`${change.kind}-${change.id}-${change.from}`)}
        <li class="change">
          <button class="go" title="Go to this change" onclick={() => session.goTo(change.from, change.to)}>
            <span class="kind {change.kind}">{change.kind === "insertion" ? "Added" : "Deleted"}</span>
            <span class="what">“{change.text.trim() || " "}”</span>
            <span class="who">{change.author || "Someone"} {date(change.date)}</span>
          </button>
          <span class="actions">
            <button class="button" title={tooltipFor("wAcceptChange")} onclick={() => session.resolveChange(change, true)}>{tools.wAcceptChange.label}</button>
            <button class="button" title={tooltipFor("wRejectChange")} onclick={() => session.resolveChange(change, false)}>{tools.wRejectChange.label}</button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<section aria-labelledby="w-comments">
  <h3 id="w-comments" title={tooltipFor("wComments")}>{tools.wComments.label}</h3>
  <div class="setting">
    <textarea class="field" rows="2" placeholder={hasSelection ? "Comment on the selected text" : "Select text, then write a comment"} bind:value={newComment}></textarea>
    <button
      class="button"
      class:primary={hasSelection && !!newComment.trim()}
      disabled={!hasSelection || !newComment.trim()}
      title={tooltipFor("wInsertComment")}
      onclick={() => {
        if (session.addComment(newComment.trim())) newComment = "";
      }}>Add Comment</button
    >
  </div>
  {#if comments.length === 0}
    <p class="hint">No comments.</p>
  {:else}
    <ul class="list" aria-label="Comments">
      {#each comments as comment (comment.id)}
        <li class="comment" class:focused={focusComment === comment.id}>
          <button
            class="go"
            title="Go to the text this comment is about"
            onclick={() => {
              const at = commentPosition(comment.id);
              if (at) session.goTo(at.from, at.to);
            }}
          >
            <span class="who">{comment.author || "Someone"} {date(comment.date)}</span>
          </button>
          <textarea
            class="field"
            rows="2"
            aria-label={`Comment by ${comment.author}`}
            value={commentText(comment.blocks as never)}
            onchange={(event) => {
              const text = event.currentTarget.value;
              session.run((s) => edit.editComment(s, comment.id, text), false);
            }}
          ></textarea>
          <button class="button delete" title="Delete this comment" onclick={() => session.run((s) => edit.deleteComment(s, comment.id), false)}>Delete</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    padding: 0.75rem 0.25rem;
    border-top: 0.5px solid var(--separator);
  }

  section:first-child {
    border-top: 0;
  }

  h3 {
    margin: 0;
    font: var(--headline);
  }

  .row {
    display: flex;
    gap: 0.75rem;
    align-items: center;
  }

  .buttons {
    display: flex;
    gap: 0.375rem;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .change,
  .comment {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .comment.focused {
    box-shadow: 0 0 0 1px var(--accent);
  }

  .go {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 0.5rem;
    padding: 0;
    border: 0;
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .kind {
    font: var(--subheadline);
    font-weight: 600;
  }

  .kind.insertion {
    color: #1a6fd4;
  }

  .kind.deletion {
    color: #c4314b;
  }

  .what {
    overflow: hidden;
    max-width: 100%;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .who {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .actions {
    display: flex;
    gap: 0.25rem;
  }

  textarea.field {
    width: 100%;
    resize: vertical;
    font: var(--body);
  }

  .delete {
    align-self: flex-start;
  }
</style>
