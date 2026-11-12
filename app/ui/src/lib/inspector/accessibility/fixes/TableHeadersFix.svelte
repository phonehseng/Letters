<script lang="ts">
  // a table's header rows and columns: the table itself, and how many of
  // its first rows and columns label the others. screen readers then say
  // the header with each cell ("Year 2025: 14 students").
  import type { Finding, FixKind } from "../../../a11y/a11yApi";
  import type { A11ySession } from "../../../a11y/a11ySession.svelte";
  import type { ToolContext } from "../../../inspector";
  import RegionPicture from "./RegionPicture.svelte";

  interface Props {
    context: ToolContext;
    session: A11ySession;
    finding: Finding;
    fix: Extract<FixKind, { kind: "setTableHeaders" }>;
  }

  let { context, session, finding, fix }: Props = $props();

  // the engine's suggestion fills the fields once
  // svelte-ignore state_referenced_locally
  let rows = $state(fix.suggestedHeaderRows);
  // svelte-ignore state_referenced_locally
  let columns = $state(fix.suggestedHeaderColumns);
  let busy = $state(false);
  const valid = $derived(rows >= 0 && columns >= 0 && rows + columns > 0);

  async function apply() {
    busy = true;
    await session.fix({ kind: "setTableHeaders", table: fix.table, headerRows: rows, headerColumns: columns });
    busy = false;
  }

  function describe(count: number, one: string, many: string): string {
    return count === 0 ? `no ${many}` : count === 1 ? `the first ${one}` : `the first ${count} ${many}`;
  }
</script>

{#if finding.where.page !== null && finding.where.rect}
  <RegionPicture
    documentId={context.document.id}
    revision={context.document.revision}
    page={finding.where.page}
    rect={finding.where.rect}
    label={`The table on page ${finding.where.page + 1}`}
  />
{/if}
<div class="setting-row">
  <label class="setting">
    <span class="caption">Header rows</span>
    <input class="field" type="number" min="0" max="10" bind:value={rows} />
  </label>
  <label class="setting">
    <span class="caption">Header columns</span>
    <input class="field" type="number" min="0" max="10" bind:value={columns} />
  </label>
</div>
<div class="actions">
  <button class="button primary" disabled={!valid || busy} onclick={apply}>Set Headers</button>
</div>
<p class="hint">
  {valid
    ? `Makes ${describe(rows, "row", "rows")} and ${describe(columns, "column", "columns")} the headers. Undo with Ctrl Z.`
    : "Choose at least one header row or column."}
</p>

<style>
  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
