<script lang="ts">
  // the heading ladder: every heading with the level the engine proposes,
  // each one changeable, so headings go H1, H2, H3 without skipping.
  // applying sends only the headings whose level changes, as one edit.
  import { headingProposal } from "../../../a11y/a11yApi";
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import { ladderChanges, ladderFrom, setLevel, type LadderRow } from "../../../a11y/ladder";
  import type { ToolContext } from "../../../inspector";

  interface Props {
    context: ToolContext;
    session: A11ySession;
  }

  let { context, session }: Props = $props();

  let rows = $state<LadderRow[]>([]);
  let loaded = $state(false);
  let busy = $state(false);
  const changes = $derived(ladderChanges(rows));

  $effect(() => {
    headingProposal(context.document.id)
      .then((proposal) => (rows = ladderFrom(proposal)))
      .catch(() => (rows = []))
      .finally(() => (loaded = true));
  });

  async function apply() {
    busy = true;
    await session.fix({ kind: "setHeadingLevels", levels: changes });
    busy = false;
  }
</script>

{#if !loaded}
  <p class="hint">Reading the headings…</p>
{:else if rows.length === 0}
  <p class="hint">No heading needs another level.</p>
{:else}
  <ol class="ladder" aria-label="Headings and their levels">
    {#each rows as row, index (row.element.number)}
      <li style:padding-left="{(row.to - 1) * 0.625}rem">
        <label>
          <select class="field" value={row.to} onchange={(event) => (rows = setLevel(rows, index, Number(event.currentTarget.value)))} aria-label={`Level of “${row.text}”`}>
            {#each [1, 2, 3, 4, 5, 6] as level (level)}
              <option value={level}>H{level}</option>
            {/each}
          </select>
          <span class="text">{row.text}</span>
        </label>
        {#if row.from !== row.to}
          <span class="hint">was H{row.from}</span>
        {/if}
      </li>
    {/each}
  </ol>
  <div class="actions">
    <button class="button primary" disabled={changes.length === 0 || busy} onclick={apply}>Apply Headings</button>
  </div>
  <p class="hint">Changes {changes.length === 1 ? "1 heading" : `${changes.length} headings`}. The text and its look stay the same; only the tags change. Undo with Ctrl Z.</p>
{/if}

<style>
  .ladder {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.375rem;
  }

  label {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    min-width: 0;
  }

  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
