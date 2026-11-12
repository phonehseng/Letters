<script lang="ts">
  // the review before applying: exactly what each mark will remove, page
  // by page, the engine's warnings in plain words, and the page as it is
  // and as it will be. the "after" picture is a redacted copy made by a
  // dry run in the shell; the working copy doesn't change until Apply.
  import { tileUrl, type DocumentState } from "../api";
  import { markLabel } from "./pendingMarks";
  import ProtectSheet from "./ProtectSheet.svelte";
  import type { ProtectSession } from "./protectSession.svelte";
  import { coversNothing, itemSummary, picturesNote, reviewTotals, totalsSentence, wholesaleNote } from "./review";

  interface Props {
    session: ProtectSession;
    document: DocumentState;
  }

  let { session, document: doc }: Props = $props();

  const review = $derived(session.review);
  const totals = $derived(reviewTotals(review?.pages ?? []));
  const pages = $derived((review?.pages ?? []).map((page) => page.page));
  const shown = $derived(review?.shownPage ?? 0);
  // one 512 pixel tile holds the whole page at this scale
  const size = $derived(doc.pages[shown]);
  const renderScale = $derived(size ? Math.min(512 / size.width, 512 / size.height) : 1);
  const cssWidth = $derived(size ? Math.round((size.width * renderScale) / 2) : 0);
  const scaleMilli = $derived(Math.floor(renderScale * 1000));

  function labelFor(markId: string): string {
    const mark = session.marks.find((each) => each.id === markId);
    return mark ? markLabel(mark) : "";
  }

  function choosePage(page: number) {
    if (session.review) session.review = { ...session.review, shownPage: page };
  }

  function apply() {
    session.closeReview();
    session.askToApply();
  }
</script>

<ProtectSheet
  id="review-redactions"
  role="dialog"
  wide
  title="Review Redactions"
  message="This is exactly what Apply Redactions removes. Nothing has changed yet."
  onCancel={() => session.closeReview()}
>
  {#if review?.loading}
    <p class="hint" role="status">Checking what the marks cover…</p>
  {:else if review?.error}
    <p class="hint" role="alert">{review.error}</p>
  {:else if review}
    <p class="totals" role="status">{totalsSentence(totals)}</p>
    <div class="columns">
      <div class="list">
        {#each review.pages as page (page.page)}
          <section aria-labelledby={`review-page-${page.page}`}>
            <h3 id={`review-page-${page.page}`}>Page {page.page + 1}</h3>
            {#if wholesaleNote(page)}
              <p class="warning">{wholesaleNote(page)}</p>
            {/if}
            {#if picturesNote(page)}
              <p class="hint">{picturesNote(page)}</p>
            {/if}
            <ul>
              {#each page.items as item (item.markId)}
                <li class:empty={coversNothing(item)}>
                  {#if item.text.length > 0}
                    <span class="removed-text">{item.text.join(" ")}</span>
                  {:else}
                    <span class="what">{labelFor(item.markId)}</span>
                  {/if}
                  {#if itemSummary(item)}
                    <span class="hint">{itemSummary(item)}</span>
                  {/if}
                  {#each [...item.comments, ...item.formValues] as extra, index (index)}
                    <span class="hint">Also removed: {extra}</span>
                  {/each}
                  <span class="row-actions">
                    <button class="button" onclick={() => choosePage(item.page)}>Show Page</button>
                    <button class="button" onclick={() => session.removeMark(item.markId)}>Remove Mark</button>
                  </span>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
        {#if review.warnings.length > 0}
          <section aria-labelledby="review-warnings">
            <h3 id="review-warnings">Still in the document</h3>
            <ul>
              {#each review.warnings as warning, index (index)}
                <li>
                  <span>{warning.text}</span>
                  {#if warning.phrase}
                    <span class="row-actions">
                      <button class="button" onclick={() => session.findElsewhere(warning.phrase ?? "")}>Find and Mark…</button>
                    </span>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      </div>
      {#if review.dryRun && size}
        <figure class="compare" aria-label={`Page ${shown + 1} before and after`}>
          {#if pages.length > 1}
            <label class="setting-row">
              <span class="caption">Page</span>
              <select class="field" value={shown} onchange={(event) => choosePage(Number(event.currentTarget.value))}>
                {#each pages as page (page)}
                  <option value={page}>Page {page + 1}</option>
                {/each}
              </select>
            </label>
          {/if}
          <div class="pictures">
            <div>
              <img
                src={tileUrl(doc.id, shown, scaleMilli, 0, 0, doc.revision)}
                width={cssWidth}
                alt={`Page ${shown + 1} now, with the content still there`}
              />
              <span class="under">Now</span>
            </div>
            <div>
              <img
                src={tileUrl(review.dryRun.previewId, shown, scaleMilli, 0, 0, 0)}
                width={cssWidth}
                alt={`Page ${shown + 1} after redacting`}
              />
              <span class="under">After</span>
            </div>
          </div>
        </figure>
      {/if}
    </div>
  {/if}
  {#snippet buttons()}
    <button class="button" data-autofocus onclick={() => session.closeReview()}>Close</button>
    <button class="button primary" disabled={!review || review.loading || session.marks.length === 0} onclick={apply}>Apply Redactions…</button>
  {/snippet}
</ProtectSheet>

<style>
  .totals {
    margin: 0;
  }

  .columns {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  .list {
    flex: 1 1 16rem;
    min-width: 0;
  }

  h3 {
    margin: 0.5rem 0 0.25rem;
    font: var(--headline);
  }

  ul {
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
    gap: 0.125rem;
    padding: 0.5rem 0.625rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }

  li.empty {
    outline: 1px dashed var(--label-tertiary);
  }

  .removed-text {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .row-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.375rem;
    margin-top: 0.25rem;
  }

  .warning {
    margin: 0 0 0.375rem;
  }

  .compare {
    flex: 0 1 auto;
    margin: 0;
  }

  .pictures {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  img {
    display: block;
    height: auto;
    box-shadow: var(--shadow-sheet);
    background: #ffffff;
  }

  .under {
    display: block;
    margin-top: 0.25rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    text-align: center;
  }
</style>
