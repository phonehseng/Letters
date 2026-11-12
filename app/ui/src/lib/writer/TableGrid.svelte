<script lang="ts">
  // the table picker at the top of the Insert menu: point at a grid to
  // choose rows and columns (or use the arrow keys), click to add it.
  import { tooltipFor } from "../tools";

  interface Props {
    onPick: (rows: number, columns: number) => void;
  }

  let { onPick }: Props = $props();
  const SIZE = 8;
  let rows = $state(2);
  let columns = $state(2);

  function onKey(event: KeyboardEvent) {
    const moves: Record<string, [number, number]> = { ArrowRight: [0, 1], ArrowLeft: [0, -1], ArrowDown: [1, 0], ArrowUp: [-1, 0] };
    const move = moves[event.key];
    if (move) {
      event.preventDefault();
      event.stopPropagation();
      rows = Math.min(SIZE, Math.max(1, rows + move[0]));
      columns = Math.min(SIZE, Math.max(1, columns + move[1]));
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onPick(rows, columns);
    }
  }
</script>

<div class="table-grid">
  <div class="caption">Table: {rows} × {columns}</div>
  <div
    class="grid"
    role="slider"
    tabindex="0"
    data-autofocus
    aria-label="Table size"
    aria-valuetext={`${rows} rows by ${columns} columns. Arrow keys change it, Enter adds the table.`}
    aria-valuenow={rows * columns}
    title={tooltipFor("wInsertTable")}
    onkeydown={onKey}
  >
    {#each Array(SIZE) as _, row (row)}
      {#each Array(SIZE) as __, column (column)}
        <button
          class="cell"
          class:on={row < rows && column < columns}
          tabindex="-1"
          aria-hidden="true"
          onpointerenter={() => ((rows = row + 1), (columns = column + 1))}
          onclick={() => onPick(row + 1, column + 1)}
        ></button>
      {/each}
    {/each}
  </div>
  <div class="separator"></div>
</div>

<style>
  .caption {
    padding: 0.25rem 0.5rem;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(8, 1rem);
    gap: 0.1875rem;
    padding: 0.25rem 0.5rem 0.5rem;
    border-radius: var(--radius-small);
  }

  .cell {
    width: 1rem;
    height: 1rem;
    padding: 0;
    border: 0.5px solid var(--separator);
    border-radius: 0.1875rem;
    background: var(--control);
  }

  .cell.on {
    background: var(--accent-soft);
    border-color: var(--accent);
  }

  .separator {
    height: 0.5px;
    margin: 0.25rem 0.5rem;
    background: var(--separator);
  }
</style>
