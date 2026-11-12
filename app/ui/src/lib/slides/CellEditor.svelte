<script module lang="ts">
  // typing in a table cell (the component below), and the grid maths it and
  // the canvas share: a cell's box, the cell under a point, the next cell.
  import type { TableView } from "./slidesApi";

  /** a cell's box in the table's own points, spans included. */
  export function cellBox(table: TableView, row: number, column: number) {
    const sum = (values: number[]) => values.reduce((total, value) => total + value, 0);
    const cell = table.rows[row]?.cells[column];
    const columnSpan = Math.max(1, cell?.columnSpan ?? 1);
    const rowSpan = Math.max(1, cell?.rowSpan ?? 1);
    return {
      x: sum(table.columns.slice(0, column)),
      y: sum(table.rows.slice(0, row).map((each) => each.height)),
      width: sum(table.columns.slice(column, column + columnSpan)),
      height: sum(table.rows.slice(row, row + rowSpan).map((each) => each.height)),
    };
  }

  /** the cell under a point of the table (its own points), skipping covered cells. */
  export function cellAt(table: TableView, x: number, y: number): { row: number; column: number } | null {
    for (let row = 0; row < table.rows.length; row++) {
      for (let column = 0; column < table.columns.length; column++) {
        if (table.rows[row].cells[column]?.covered) continue;
        const box = cellBox(table, row, column);
        if (x >= box.x && x < box.x + box.width && y >= box.y && y < box.y + box.height) return { row, column };
      }
    }
    return null;
  }

  /** the next cell for Tab (or Shift Tab), skipping covered ones; null past the ends. */
  export function nextCell(table: TableView, row: number, column: number, backwards: boolean): { row: number; column: number } | null {
    const columns = table.columns.length;
    let index = row * columns + column;
    for (;;) {
      index += backwards ? -1 : 1;
      if (index < 0 || index >= table.rows.length * columns) return null;
      const next = { row: Math.floor(index / columns), column: index % columns };
      if (!table.rows[next.row].cells[next.column]?.covered) return next;
    }
  }
</script>

<script lang="ts">
  // typing in a table cell: a field over the cell while the slide is drawn
  // without that cell's words. Tab and Shift Tab keep the words and move to
  // the next or previous cell, as in powerpoint; Escape keeps them and stops.
  import { onMount, untrack } from "svelte";
  import type { ObjectView } from "./slidesApi";

  interface Props {
    object: ObjectView;
    row: number;
    column: number;
    scale: number;
    /** the theme's body font, which table text uses unless the file says otherwise */
    font: string;
    onDone: (text: string | null, move: "next" | "previous" | null) => void;
  }

  let { object, row, column, scale, font, onDone }: Props = $props();
  // a field is made for one cell; the canvas makes a new one for the next (keyed by cell)
  const table = untrack(() => object.table!);
  const box = untrack(() => cellBox(table, row, column));
  const original = untrack(() => table.rows[row]?.cells[column]?.text ?? "");
  let words = $state(original);
  let field = $state<HTMLTextAreaElement>();
  let finished = false;

  /** powerpoint's table text is 18 pt unless the table style says otherwise */
  const TABLE_TEXT_POINTS = 18;
  /** a cell's default inset, 0.1 inch at the sides and 0.05 inch above and below */
  const INSET_SIDE = 7.2;
  const INSET_TOP = 3.6;

  function finish(move: "next" | "previous" | null) {
    if (finished) return;
    finished = true;
    onDone(words === original ? null : words, move);
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Tab") {
      event.preventDefault();
      finish(event.shiftKey ? "previous" : "next");
    } else if (event.key === "Escape") {
      event.preventDefault();
      finish(null);
    }
  }

  // the cell's words are selected, as Tab selects them in powerpoint, so typing replaces them
  onMount(() => {
    field?.focus();
    field?.select();
  });
</script>

<div
  class="table"
  style:left="{object.x * scale}px"
  style:top="{object.y * scale}px"
  style:width="{object.width * scale}px"
  style:height="{object.height * scale}px"
  style:transform="rotate({object.rotation}deg)"
>
  <textarea
    bind:this={field}
    bind:value={words}
    aria-label="Row {row + 1}, column {column + 1} of {object.name}. Tab goes to the next cell."
    style:left="{box.x * scale}px"
    style:top="{box.y * scale}px"
    style:width="{box.width * scale}px"
    style:height="{box.height * scale}px"
    style:padding="{INSET_TOP * scale}px {INSET_SIDE * scale}px"
    style:font-family="'{font}', sans-serif"
    style:font-size="{TABLE_TEXT_POINTS * scale}px"
    onkeydown={onKeyDown}
    onblur={() => finish(null)}
    onpointerdown={(event) => event.stopPropagation()}
  ></textarea>
</div>

<style>
  .table {
    position: absolute;
    z-index: 3;
    pointer-events: none;
  }

  textarea {
    position: absolute;
    box-sizing: border-box;
    margin: 0;
    border: 0;
    outline: 2px solid var(--accent);
    background: transparent;
    color: inherit;
    line-height: 1.2;
    resize: none;
    pointer-events: auto;
  }
</style>
