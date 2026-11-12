<script lang="ts">
  // Go To: a small capsule under the toolbar that takes "p 12", "xii",
  // "50%", "ch 3", "#methods", "last" and lists what fits as you type.
  // with nothing typed it lists the bookmarks. Enter goes to the first.
  import { onMount } from "svelte";
  import { tools, tooltipFor } from "../tools";
  import { noPlaceSentence, resolveJump, type JumpCandidate, type JumpDocument } from "./jump";

  interface Props {
    document: JumpDocument;
    top: number;
    onJump: (candidate: JumpCandidate) => void;
    onAddBookmark: () => void;
    onRemoveBookmark: (page: number, label: string) => void;
    onClose: () => void;
  }

  let { document, top, onJump, onAddBookmark, onRemoveBookmark, onClose }: Props = $props();

  let field = $state<HTMLInputElement>();
  let typed = $state("");
  let current = $state(0);

  const bookmarkRows = $derived<JumpCandidate[]>(document.bookmarks.map((b) => ({ kind: "bookmark", page: b.page, block: null, label: b.title })));
  const candidates = $derived(typed.trim() ? resolveJump(document, typed) : bookmarkRows);
  const live = $derived(typed.trim() ? (candidates.length === 0 ? noPlaceSentence(document, typed) : candidates.length === 1 ? "1 place" : `${candidates.length} places`) : "");

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      current = Math.max(0, Math.min(candidates.length - 1, current + step));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const chosen = candidates[current];
      if (chosen) onJump(chosen);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }

  function kindWord(candidate: JumpCandidate): string {
    const words: Record<string, string> = { outline: "Heading", bookmark: "Bookmark", label: "Printed Page", lastPosition: "Last Read", percent: "Position", relative: "Move", word: "Word" };
    return words[candidate.kind] ?? "Page";
  }

  onMount(() => field?.focus());
</script>

<div class="glass goto" role="dialog" aria-label="Go To" style:top="{top}px">
  <input
    bind:this={field}
    bind:value={typed}
    oninput={() => (current = 0)}
    onkeydown={onKeyDown}
    placeholder="Page, xii, 50%, ch 3, #heading"
    aria-label="Go to"
    aria-controls="goto-list"
    aria-activedescendant={candidates.length ? `goto-${current}` : undefined}
    spellcheck="false"
    autocomplete="off"
  />
  <span class="visually-hidden" aria-live="polite">{live}</span>
  {#if candidates.length}
    <ul id="goto-list" role="listbox" aria-label={typed.trim() ? "Places" : "Bookmarks"}>
      {#each candidates as candidate, i (i)}
        <li id="goto-{i}" role="option" aria-selected={i === current} class:current={i === current}>
          <button class="place" tabindex="-1" onclick={() => onJump(candidate)}>
            <span class="kind">{kindWord(candidate)}</span>
            <span class="label">{candidate.label}</span>
            {#if candidate.kind !== "page" && candidate.kind !== "label"}<span class="page">p. {candidate.page + 1}</span>{/if}
          </button>
          {#if !typed.trim()}
            <button class="remove" aria-label={`Remove bookmark ${candidate.label}`} onclick={() => onRemoveBookmark(candidate.page, candidate.label)}>Remove</button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if !typed.trim()}
    <p class="hint">No bookmarks yet.</p>
  {:else}
    <!-- nothing matches: say why rather than show an empty list -->
    <p class="hint">{noPlaceSentence(document, typed)}</p>
  {/if}
  <div class="footer">
    <button class="button" title={tooltipFor("addBookmark")} onclick={onAddBookmark}>{tools.addBookmark.label}</button>
  </div>
</div>

<style>
  .goto {
    position: absolute;
    left: 50%;
    z-index: 2;
    width: min(24rem, calc(100% - 1.5rem));
    padding: 0.5rem;
    border-radius: 1.125rem;
    transform: translateX(-50%);
    margin-top: 0.5rem;
  }

  input {
    width: 100%;
    height: 2rem;
    padding: 0 0.75rem;
    border: 0;
    border-radius: 0.875rem;
    outline: 0;
    background: var(--control);
    color: var(--label);
    font: var(--body);
  }

  ul {
    max-height: 18rem;
    margin: 0.5rem 0 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    border-radius: var(--radius-small);
  }

  li.current {
    background: var(--accent-soft);
  }

  .place {
    display: flex;
    flex: 1;
    min-width: 0;
    gap: 0.5rem;
    align-items: baseline;
    padding: 0.375rem 0.5rem;
    border: 0;
    background: transparent;
    color: var(--label);
    text-align: left;
    font: var(--body);
  }

  .kind,
  .page,
  .hint {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .remove {
    border: 0;
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .footer {
    display: flex;
    justify-content: flex-end;
    margin-top: 0.5rem;
  }
</style>
