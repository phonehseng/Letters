<script lang="ts">
  // the writer's notice bar under the toolbar (the recovered version, the
  // session's messages with their one action, the question before saving
  // as .odt) and the small sheets that ask for a link or a comment.
  import { tools } from "../tools";
  import * as edit from "./editing";
  import PromptSheet from "./PromptSheet.svelte";
  import type { WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    /** the toolbar's height: everything sits just under it */
    top: number;
    recovery: { savedAt: number } | null;
    onRecover: () => void;
    onDismissRecovery: () => void;
    /** what saving as .odt would lose, asked before saving to `path` */
    odtQuestion: { text: string; path: string } | null;
    prompt: "link" | "comment" | null;
  }

  let { session, top, recovery, onRecover, onDismissRecovery, odtQuestion = $bindable(), prompt = $bindable() }: Props = $props();
  const editor = $derived(session.editor);
</script>

{#if recovery}
  <div class="glass notice" role="alertdialog" aria-label="Recovered version" style:top="{top + 8}px">
    <span>Letters kept unsaved changes to this document from {new Date(recovery.savedAt * 1000).toLocaleString()}, when it last closed without saving.</span>
    <button class="button" onclick={onDismissRecovery}>Discard</button>
    <button class="button primary" onclick={onRecover}>Recover</button>
  </div>
{:else if session.notice}
  <div class="glass notice" role="status" style:top="{top + 8}px">
    <span>{session.notice.text}</span>
    {#if session.notice.action}
      <button
        class="button"
        onclick={() => {
          const action = session.notice?.action;
          session.notice = null;
          action?.run();
        }}>{session.notice.action.label}</button
      >
    {/if}
    <button class="button" onclick={() => (session.notice = null)}>OK</button>
  </div>
{/if}

{#if odtQuestion}
  <div class="glass notice" role="alertdialog" aria-label="Save as OpenDocument" style:top="{top + 8}px">
    <span>{odtQuestion.text}</span>
    <button class="button" onclick={() => (odtQuestion = null)}>Cancel</button>
    <button
      class="button primary"
      onclick={() => {
        const path = odtQuestion?.path;
        odtQuestion = null;
        if (path) session.save(path);
      }}>Save as ODT</button
    >
  </div>
{/if}

{#if prompt === "link"}
  <PromptSheet
    title={tools.wInsertLink.label.replace("…", "")}
    label="Web address"
    placeholder="https://"
    value={String(edit.markData(editor, editor.schema.marks.link)?.href ?? "")}
    confirm="Add Link"
    {top}
    secondary={edit.markActive(editor, editor.schema.marks.link) ? { label: "Remove Link", run: () => ((prompt = null), session.setLink(null)) } : undefined}
    onSubmit={(href) => {
      prompt = null;
      session.setLink(/^[a-z]+:/i.test(href) ? href : `https://${href}`);
      session.view?.focus();
    }}
    onCancel={() => ((prompt = null), session.view?.focus())}
  />
{:else if prompt === "comment"}
  <PromptSheet
    title="Comment"
    label="Your comment on the selected text"
    confirm="Add Comment"
    multiline
    {top}
    onSubmit={(text) => {
      prompt = null;
      session.addComment(text);
      session.view?.focus();
    }}
    onCancel={() => ((prompt = null), session.view?.focus())}
  />
{/if}

<style>
  .notice {
    position: absolute;
    left: 50%;
    z-index: 3;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    max-width: min(36rem, calc(100% - 2rem));
    padding: 0.625rem 0.625rem 0.625rem 1rem;
    transform: translateX(-50%);
    animation: arrive 0.2s var(--ease);
  }

  .notice span {
    flex: 1 1 14rem;
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translate(-50%, -0.5rem);
    }
  }
</style>
