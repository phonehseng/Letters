<script lang="ts">
  // Remove Hidden Information: what the file carries but doesn't show,
  // each kind with how many were found, one sentence, and a box to tick.
  // what is safe to remove starts ticked; comments and form answers
  // don't, because they are often the point of the file.
  import { asCommandError } from "../../api";
  import type { ToolContext } from "../../inspector";
  import { hiddenInformation, removeHiddenInformation, type ScrubItem } from "../../protect/protectApi";
  import { scrubEffect } from "../../protect/protectEffects";
  import type { ProtectSession } from "../../protect/protectSession.svelte";
  import { countText, scrubRows, startingChoice, type ScrubRow } from "../../protect/scrubItems";
  import ToolSection from "../ToolSection.svelte";

  interface Props {
    context: ToolContext;
    session: ProtectSession;
    open: boolean;
    onToggle: () => void;
  }

  let { context, session, open, onToggle }: Props = $props();

  let rows = $state<ScrubRow[] | null>(null);
  let chosen = $state<ScrubItem[]>([]);
  let loadedAt = -1;

  // read when the section opens, and again after every change while open
  $effect(() => {
    const revision = context.document.revision;
    if (!open || revision === loadedAt) return;
    loadedAt = revision;
    hiddenInformation(context.document.id)
      .then((report) => {
        rows = scrubRows(report);
        chosen = startingChoice(rows);
      })
      .catch((problem) => {
        rows = [];
        context.showError(asCommandError(problem).message);
      });
  });

  function toggle(item: ScrubItem, on: boolean) {
    chosen = on ? [...chosen, item] : chosen.filter((each) => each !== item);
  }

  async function remove() {
    const items = [...chosen];
    const id = context.document.id;
    const worked = await context.change(async () => (await removeHiddenInformation(id, items)).document);
    if (worked) {
      context.notify("Hidden information removed from your working copy. Save the document to make it final.");
      session.refreshStatus();
    }
  }
</script>

<ToolSection
  tool="removeHiddenInfo"
  {open}
  {onToggle}
  actionLabel="Remove Selected"
  ready={chosen.length > 0}
  effect={scrubEffect(chosen.length)}
  onAction={remove}
>
  {#if rows === null}
    <p class="hint" role="status">Looking through the file…</p>
  {:else if rows.length === 0}
    <p class="hint" role="status">This file has no hidden information Letters can remove.</p>
  {:else}
    <ul class="rows" aria-label="Hidden information in this file">
      {#each rows as row (row.item)}
        <li>
          <label class="check">
            <input type="checkbox" checked={chosen.includes(row.item)} onchange={(event) => toggle(row.item, event.currentTarget.checked)} />
            <span class="text">
              <span class="name">{row.label} <span class="count">{countText(row.count)}</span></span>
              <span class="hint">{row.explanation}</span>
              {#each row.details.slice(0, 3) as detail, index (index)}
                <span class="detail">{detail}</span>
              {/each}
            </span>
          </label>
        </li>
      {/each}
    </ul>
  {/if}
</ToolSection>

<style>
  .rows {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    min-width: 0;
  }

  .name {
    font-weight: 600;
  }

  .count {
    color: var(--label-secondary);
    font-weight: 400;
  }

  .detail {
    overflow: hidden;
    color: var(--label-secondary);
    font: var(--subheadline);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
