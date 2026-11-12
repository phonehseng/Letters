<script lang="ts">
  // Find and Mark: a phrase, or one click for email addresses, phone
  // numbers or long numbers. the matches are listed with their page and
  // the words around them, each with a box to tick (all ticked to start),
  // and the button marks the ticked ones. the boxes come from the same
  // reading of the page that redaction uses, so a match covers exactly
  // the letters redaction removes.
  import { untrack } from "svelte";
  import { asCommandError } from "../../api";
  import { pageText } from "../../annotate/annotateApi";
  import type { ToolContext } from "../../inspector";
  import { describePages } from "../../pageRanges";
  import type { PageText } from "../../pageTools/textSelection";
  import { asItems, contextAround, foundSentence, PATTERNS, queryFor, type FoundItem, type PatternKind } from "../../protect/findMarks";
  import { findRedactionText } from "../../protect/protectApi";
  import { findEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { context, session, open, onToggle }: Props = $props();

  // the engine stops at this many matches (MAX_FOUND in core/redact)
  const MAX_FOUND = 5000;

  let phrase = $state("");
  let matchCase = $state(false);
  let items = $state<FoundItem[]>([]);
  let searched = $state(false);
  let busy = $state(false);
  let texts = $state<Record<number, PageText>>({});

  const chosen = $derived(items.filter((item) => item.chosen).length);
  const pageCount = $derived(context.document.pages.length);

  async function find(search: { phrase: string; matchCase: boolean } | { pattern: PatternKind }) {
    if ("phrase" in search && search.phrase.trim() === "") return;
    busy = true;
    try {
      items = asItems(await findRedactionText(context.document.id, queryFor(search)));
      searched = true;
      loadTexts();
    } catch (problem) {
      context.showError(asCommandError(problem).message);
    } finally {
      busy = false;
    }
  }

  /** the page text around the matches, for the context in the list. */
  async function loadTexts() {
    const id = context.document.id;
    for (const page of new Set(items.slice(0, 200).map((item) => item.page))) {
      if (texts[page]) continue;
      const text = await pageText(id, page).catch(() => null);
      if (text) texts = { ...texts, [page]: text };
    }
  }

  function markChosen() {
    const added = session.markFound(items);
    context.notify(added === 0 ? "Those matches are marked already." : `Marked ${added === 1 ? "1 area" : `${added} areas`}. Review them before applying.`);
    items = [];
    searched = false;
  }

  function setAll(value: boolean) {
    items = items.map((item) => ({ ...item, chosen: value }));
  }

  // the review's "Find and Mark…" for words left elsewhere
  let lastRequest = -1;
  $effect(() => {
    const request = session.findRequest;
    if (!request || request.at === lastRequest) return;
    lastRequest = request.at;
    untrack(() => {
      phrase = request.phrase;
      find({ phrase: request.phrase, matchCase: false });
    });
  });

  // a new version of the document makes old matches wrong
  let revision = untrack(() => context.document.revision);
  $effect(() => {
    if (context.document.revision === revision) return;
    revision = context.document.revision;
    untrack(() => {
      items = [];
      searched = false;
      texts = {};
    });
  });
</script>

<ToolSection
  tool="redactFind"
  {open}
  {onToggle}
  actionLabel={chosen === items.length && items.length > 1 ? "Mark All" : chosen === 1 ? "Mark 1 Match" : `Mark ${chosen} Matches`}
  ready={chosen > 0}
  effect={findEffect(chosen, items.length)}
  onAction={markChosen}
>
  <form
    class="search"
    onsubmit={(event) => {
      event.preventDefault();
      find({ phrase, matchCase });
    }}
  >
    <input class="field" type="search" placeholder="Word or phrase" aria-label="Word or phrase to find" bind:value={phrase} />
    <button class="button" type="submit" disabled={busy || phrase.trim() === ""}>Find</button>
  </form>
  <label class="check">
    <input type="checkbox" bind:checked={matchCase} />
    <span>Match case</span>
  </label>
  <div class="patterns" role="group" aria-label="Find a kind of text">
    {#each PATTERNS as pattern (pattern.kind)}
      <button class="button" title={pattern.description} disabled={busy} onclick={() => find({ pattern: pattern.kind })}>{pattern.label}</button>
    {/each}
  </div>
  {#if searched}
    <p class="hint" role="status">{foundSentence(items, (pages) => describePages(pages, pageCount), items.length >= MAX_FOUND)}</p>
    {#if items.length > 1}
      <div class="all">
        <button class="button" onclick={() => setAll(true)}>Tick All</button>
        <button class="button" onclick={() => setAll(false)}>Tick None</button>
      </div>
    {/if}
    {#if items.length > 0}
      <ul class="matches" aria-label="Matches">
        {#each items.slice(0, 500) as item, index (item.key)}
          {@const around = contextAround(texts[item.page], item.rects)}
          <li>
            <label class="check">
              <input type="checkbox" bind:checked={items[index].chosen} />
              <span>
                <span class="page">Page {item.page + 1}</span>
                <span class="context">{around.before}<strong>{item.text}</strong>{around.after}</span>
              </span>
            </label>
          </li>
        {/each}
      </ul>
      {#if items.length > 500}
        <p class="hint">The first 500 are listed. Mark All marks every ticked match, listed or not.</p>
      {/if}
    {/if}
  {/if}
</ToolSection>

<style>
  .search {
    display: flex;
    gap: 0.375rem;
  }

  .search .field {
    flex: 1;
    min-width: 0;
  }

  .patterns,
  .all {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
  }

  .matches {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 16rem;
    margin: 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }

  .matches li {
    padding: 0.25rem 0.375rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  .page {
    display: block;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .context {
    overflow-wrap: anywhere;
  }
</style>
