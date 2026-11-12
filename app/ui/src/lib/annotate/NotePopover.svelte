<script lang="ts">
  // the pop-up beside a sticky note: type the comment, read the replies,
  // answer, or delete the note. a new note becomes an annotation when it
  // is saved; closing an empty new note leaves the page as it was.
  //
  // Ctrl Enter (Command Return on a mac) saves, Escape closes.
  import { tick } from "svelte";
  import { popoverSpot } from "./annotations";
  import type { AnnotateSession } from "./annotateSession.svelte";
  import { commentByline, commentThreads } from "./comments";
  import type { PageFrame } from "../pageTools/pagePoints";
  import { NOTE_ICON_SIZE } from "./annotateSession.svelte";

  interface Props {
    session: AnnotateSession;
    frame: PageFrame;
  }

  let { session, frame }: Props = $props();

  const POPOVER_WIDTH_PX = 256;

  const draft = $derived(session.note!);
  const isNew = $derived(draft.id === null);
  const thread = $derived(draft.id ? commentThreads(session.annotations).find((each) => each.root.id === draft.id) : undefined);
  const spot = $derived(
    popoverSpot(
      { x: draft.position.x, y: draft.position.y, width: NOTE_ICON_SIZE, height: NOTE_ICON_SIZE },
      frame.size,
      POPOVER_WIDTH_PX / frame.scale,
    ),
  );
  const nowSeconds = Date.now() / 1000;

  let text = $state("");
  let reply = $state("");
  let busy = $state(false);
  let field = $state<HTMLTextAreaElement>();
  // the draft whose words are in the field, so another note's words replace them
  let shownFor: string | null | undefined = undefined;

  $effect(() => {
    const key = draft.id ?? `new-${draft.position.x}-${draft.position.y}`;
    if (key === shownFor) return;
    shownFor = key;
    text = draft.text;
    reply = "";
    if (isNew) tick().then(() => field?.focus());
  });

  // the session saves these words if a click elsewhere closes the note
  $effect(() => {
    session.noteTyped = text;
    return () => (session.noteTyped = null);
  });

  async function save() {
    if (busy) return;
    busy = true;
    try {
      if (isNew && text.trim() === "") {
        session.note = null;
        return;
      }
      await session.saveNote(text);
    } finally {
      busy = false;
    }
  }

  async function sendReply() {
    if (!thread || reply.trim() === "" || busy) return;
    busy = true;
    try {
      if (await session.reply(thread.root, reply.trim())) reply = "";
    } finally {
      busy = false;
    }
  }

  function close() {
    session.note = null;
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      if ((event.target as HTMLElement).classList.contains("reply")) sendReply();
      else save();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="glass popover"
  role="dialog"
  tabindex="-1"
  aria-label={isNew ? "New note" : `Note on page ${draft.page + 1}`}
  data-page-ui
  style:left="{frame.left + spot.x * frame.scale}px"
  style:top="{frame.top + spot.y * frame.scale}px"
  style:width="{POPOVER_WIDTH_PX}px"
  onkeydown={onKeyDown}
  onpointerdown={(event) => event.stopPropagation()}
>
  {#if thread}
    <p class="byline">{commentByline(thread.root, nowSeconds, false)}</p>
  {/if}
  <label class="visually-hidden" for="note-text">Comment</label>
  <textarea
    id="note-text"
    class="field"
    rows="3"
    placeholder="Write a comment"
    bind:this={field}
    bind:value={text}
  ></textarea>
  <div class="actions">
    {#if !isNew && draft.id}
      <button class="button" title="Delete this note and its replies" onclick={() => session.deleteById(draft.page, draft.id!)}>Delete</button>
    {/if}
    <span class="spacer"></span>
    <button class="button" onclick={close}>{isNew ? "Cancel" : "Close"}</button>
    <button class="button primary" disabled={busy || (!isNew && text === draft.text)} onclick={save}>
      {isNew ? "Add Note" : "Save"}
    </button>
  </div>

  {#if thread}
    {#if thread.replies.length > 0}
      <ol class="replies" aria-label="Replies">
        {#each thread.replies as answer (answer.id)}
          <li>
            <p class="byline">{commentByline(answer, nowSeconds, false)}</p>
            <p class="words">{answer.contents}</p>
          </li>
        {/each}
      </ol>
    {/if}
    <label class="visually-hidden" for="note-reply">Reply</label>
    <textarea id="note-reply" class="field reply" rows="2" placeholder="Reply" bind:value={reply}></textarea>
    <div class="actions">
      <button class="button" disabled={busy || reply.trim() === ""} onclick={sendReply}>Reply</button>
    </div>
  {/if}
</div>

<style>
  .popover {
    position: absolute;
    z-index: 2;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem;
    animation: arrive 0.15s var(--ease);
  }

  textarea {
    width: 100%;
    resize: vertical;
    font: var(--body);
    line-height: 1.125rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.375rem;
  }

  .spacer {
    flex: 1;
  }

  .byline {
    margin: 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  .replies {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin: 0;
    padding: 0.5rem 0 0;
    border-top: 0.5px solid var(--separator);
    list-style: none;
  }

  .words {
    margin: 0.125rem 0 0;
    white-space: pre-wrap;
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(0.25rem);
    }
  }
</style>
