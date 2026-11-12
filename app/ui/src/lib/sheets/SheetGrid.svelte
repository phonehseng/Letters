<script lang="ts">
  // the grid: a canvas that draws only the rows and columns on screen,
  // inside a scroll area as large as the sheet, so 200,000 rows scroll
  // like any page. the pointer selects, drags the fill handle, resizes
  // rows and columns and points at cells while a formula is typed; the
  // keyboard is the screen's (SheetsScreen.svelte). the text being typed
  // into a cell is a real text field laid exactly over the cell.
  import { onMount, untrack } from "svelte";
  import type { CellEditor } from "./cellEditor.svelte";
  import { referenceRange } from "./formulaEntry";
  import { columnCharacters, rowPoints } from "./geometry";
  import { cursorFor, fillTarget, hitTest, type Hit } from "./gridHit";
  import { GridLayout, rowHeaderWidth } from "./gridLayout";
  import { paintHeaders, paintOverlay, type Theme } from "./overlays";
  import { fontOf, paintRegion } from "./painter";
  import { addCell, extendTo, rangeOf, single, wholeColumns, wholeRows, type Selection } from "./selection";
  import type { CellRef, Range } from "./sheetsApi";
  import type { SheetView } from "./sheetView.svelte";
  import CellEditorField from "./CellEditorField.svelte";

  interface Props {
    view: SheetView;
    editor: CellEditor;
    sheetName: string;
    copied: Range | null;
    onKeyDown: (event: KeyboardEvent) => void;
    onPaste: (event: ClipboardEvent) => void;
    onEditCell: () => void;
    onFill: (source: Range, target: Range) => void;
    onResize: (axis: "rows" | "columns", indices: number[], size: number | null) => void;
    onFilterButton: (col: number) => void;
    onContextMenu: (event: MouseEvent, hit: Hit) => void;
    onEditorKey: (event: KeyboardEvent) => void;
    /** a click outside the cell being typed into finishes the entry where it is */
    onCommitEdit: () => void;
  }

  let { view, editor, sheetName, copied, onKeyDown, onPaste, onEditCell, onFill, onResize, onFilterButton, onContextMenu, onEditorKey, onCommitEdit }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  let canvas = $state<HTMLCanvasElement>();
  let width = $state(0);
  let height = $state(0);
  let scroll = $state({ x: 0, y: 0 });
  let cursor = $state("cell");
  let fillPreview = $state<Range | null>(null);
  let resizeGuide = $state<{ axis: "rows" | "columns"; at: number } | null>(null);
  let theme: Theme | null = null;
  let frame = 0;

  const headerWidth = $derived(rowHeaderWidth(view.rows.count));
  const layout = $derived(
    new GridLayout(view.rows, view.cols, view.info?.frozenRows ?? 0, view.info?.frozenCols ?? 0, scroll.x, scroll.y, width, height, headerWidth),
  );

  function readTheme(): Theme {
    const css = getComputedStyle(document.documentElement);
    const token = (name: string) => css.getPropertyValue(name).trim();
    return {
      accent: token("--accent"),
      accentSoft: token("--accent-soft"),
      header: token("--window"),
      headerText: token("--label-secondary"),
      headerLine: token("--separator"),
      headerSelected: token("--control-hover"),
      headerFont: `11px ${token("--font") || "system-ui"}`,
    };
  }

  // --- painting, once per frame however many things changed

  export function paintSoon() {
    if (frame) return;
    frame = requestAnimationFrame(paint);
  }

  /** the last paint's time and how long it took, read by the end-to-end checks */
  let lastPaint = 0;
  let paintCost = 0;

  function paint() {
    frame = 0;
    const started = performance.now();
    const context = canvas?.getContext("2d", { alpha: false });
    if (!context || !canvas || width === 0) return;
    theme ??= readTheme();
    const ratio = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
      canvas.width = Math.round(width * ratio);
      canvas.height = Math.round(height * ratio);
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    for (const range of layout.visibleRanges()) void view.cache.want(view.sheet, range);
    const sheet = { sheet: view.sheet, cache: view.cache, styles: view.styles, merged: view.info?.merged ?? [], showGridLines: view.info?.showGridLines ?? true, zoom: view.zoom };
    for (const region of layout.regions()) paintRegion(context, layout, region, sheet);
    paintOverlay(context, layout, { selection: view.selection, references: shownReferences(), copied, fillPreview, filter: view.info?.filter ?? null, placeholders: view.info?.placeholders ?? [], editing: editor.active }, theme);
    paintHeaders(context, layout, view.selection, theme);
    lastPaint = performance.now();
    paintCost = lastPaint - started;
  }

  function shownReferences() {
    if (!editor.active || editor.sheet !== view.sheet) return [];
    return editor.references.flatMap((reference) => {
      const range = referenceRange(reference, sheetName);
      return range ? [{ range, colour: reference.colour }] : [];
    });
  }

  $effect(() => {
    // anything drawn changing paints again
    void [view.paints, view.selection, layout, editor.references, editor.active, copied, fillPreview];
    untrack(paintSoon);
  });

  // --- scrolling

  function onScroll() {
    if (!scroller) return;
    scroll = { x: scroller.scrollLeft, y: scroller.scrollTop };
    // near the end of the rows offered, offer more, as the sheet goes on
    if (scroller.scrollTop + scroller.clientHeight * 3 > scroller.scrollHeight) view.reachFurther(true);
    if (scroller.scrollLeft + scroller.clientWidth * 2 > scroller.scrollWidth) view.reachFurther(false);
  }

  /** scrolls just enough to show a cell. */
  export function reveal(cell: CellRef) {
    if (!scroller) return;
    const next = layout.scrollToShow(cell);
    if (next.x !== scroll.x) scroller.scrollLeft = next.x;
    if (next.y !== scroll.y) scroller.scrollTop = next.y;
  }

  export function focus() {
    scroller?.focus({ preventScroll: true });
  }

  export function screenRows(): number {
    return Math.max(1, Math.floor((height - layout.headerHeight - layout.frozenHeight) / view.rows.defaultSize) - 1);
  }

  export function screenCols(): number {
    return Math.max(1, Math.floor((width - headerWidth - layout.frozenWidth) / view.cols.defaultSize) - 1);
  }

  export function paintedAt(): number {
    return lastPaint;
  }

  export function lastPaintCost(): number {
    return paintCost;
  }

  // --- the pointer

  function pointAt(event: PointerEvent | MouseEvent): { x: number; y: number } {
    const box = scroller!.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  }

  function currentHit(event: PointerEvent | MouseEvent): Hit {
    const { x, y } = pointAt(event);
    const last = view.selection.ranges[view.selection.ranges.length - 1];
    return hitTest(layout, x, y, editor.active ? null : last, view.info?.filter ?? null);
  }

  function onPointerMove(event: PointerEvent) {
    if (event.buttons === 0) cursor = cursorFor(currentHit(event));
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    const hit = currentHit(event);
    if (editor.active && hit.kind === "cell" && editor.wantsReference()) {
      event.preventDefault();
      startDrag(event, (cell) => editor.point(rangeOf(pointFrom ?? cell, cell), editor.sheet === view.sheet ? null : sheetName));
      pointFrom = { row: hit.row, col: hit.col };
      editor.point(rangeOf(pointFrom, pointFrom), editor.sheet === view.sheet ? null : sheetName);
      return;
    }
    if (editor.active) onCommitEdit();
    focus();
    pressOn(event, hit);
  }

  let pointFrom: CellRef | null = null;

  function pressOn(event: PointerEvent, hit: Hit) {
    switch (hit.kind) {
      case "corner":
        return view.select({ ...wholeRows(0, 1_048_575), ranges: [rangeOf({ row: 0, col: 0 }, { row: 1_048_575, col: 16_383 })] });
      case "columnHeader":
        return selectLines(event, "columns", hit.col);
      case "rowHeader":
        return selectLines(event, "rows", hit.row);
      case "columnEdge":
      case "rowEdge":
        return startResize(event, hit);
      case "fillHandle":
        return startFillDrag(event);
      case "filterButton":
        return onFilterButton(hit.col);
      case "cell":
        return pressCell(event, { row: hit.row, col: hit.col });
    }
  }

  function pressCell(event: PointerEvent, cell: CellRef) {
    let start: Selection;
    if (event.shiftKey) start = extendTo(view.selection, cell);
    else if (event.ctrlKey || event.metaKey) start = addCell(view.selection, cell);
    else start = single(cell);
    view.select(start);
    startDrag(event, (over) => view.select(extendTo(start, over)));
  }

  function selectLines(event: PointerEvent, axis: "rows" | "columns", index: number) {
    const anchorIndex = event.shiftKey ? (axis === "rows" ? view.selection.anchor.row : view.selection.anchor.col) : index;
    const make = (to: number) => (axis === "rows" ? wholeRows(Math.min(anchorIndex, to), Math.max(anchorIndex, to)) : wholeColumns(Math.min(anchorIndex, to), Math.max(anchorIndex, to)));
    view.select(make(index));
    startDrag(event, (over) => view.select(make(axis === "rows" ? over.row : over.col)));
  }

  /** follows the pointer until release, calling `onOver` with the cell under it. */
  function startDrag(event: PointerEvent, onOver: (cell: CellRef) => void, onUp?: () => void) {
    scroller?.setPointerCapture(event.pointerId);
    const move = (next: PointerEvent) => {
      const { x, y } = pointAt(next);
      const at = layout.cellAt(Math.max(x, headerWidth + 1), Math.max(y, layout.headerHeight + 1));
      onOver({ row: Math.max(0, at.row), col: Math.max(0, at.col) });
    };
    const up = () => {
      scroller?.removeEventListener("pointermove", move);
      scroller?.removeEventListener("pointerup", up);
      pointFrom = null;
      onUp?.();
    };
    scroller?.addEventListener("pointermove", move);
    scroller?.addEventListener("pointerup", up);
  }

  function startFillDrag(event: PointerEvent) {
    const source = view.selection.ranges[view.selection.ranges.length - 1];
    startDrag(
      event,
      (over) => (fillPreview = fillTarget(source, over)),
      () => {
        const target = fillPreview;
        fillPreview = null;
        if (target) onFill(source, rangeOf(target.start, target.end));
      },
    );
  }

  function startResize(event: PointerEvent, hit: Hit) {
    const axis = hit.kind === "columnEdge" ? "columns" : "rows";
    const index = hit.kind === "columnEdge" ? hit.col : hit.kind === "rowEdge" ? hit.row : 0;
    const startAt = axis === "columns" ? layout.xOf(index) : layout.yOf(index);
    scroller?.setPointerCapture(event.pointerId);
    const move = (next: PointerEvent) => {
      const point = pointAt(next);
      resizeGuide = { axis, at: Math.max(startAt + 2, axis === "columns" ? point.x : point.y) };
    };
    const up = () => {
      scroller?.removeEventListener("pointermove", move);
      scroller?.removeEventListener("pointerup", up);
      const guide = resizeGuide;
      resizeGuide = null;
      if (!guide) return;
      const pixels = (guide.at - startAt) / view.zoom;
      onResize(axis, linesToResize(axis, index), axis === "columns" ? columnCharacters(pixels) : rowPoints(pixels));
    };
    scroller?.addEventListener("pointermove", move);
    scroller?.addEventListener("pointerup", up);
  }

  /** dragging the edge of a selected header resizes every selected line, as in excel */
  function linesToResize(axis: "rows" | "columns", index: number): number[] {
    const range = view.selection.ranges[view.selection.ranges.length - 1];
    const [first, last] = axis === "columns" ? [range.start.col, range.end.col] : [range.start.row, range.end.row];
    const whole = axis === "columns" ? range.start.row === 0 && range.end.row === 1_048_575 : range.start.col === 0 && range.end.col === 16_383;
    if (!whole || index < first || index > last || last - first > 500) return [index];
    return Array.from({ length: last - first + 1 }, (_, i) => first + i);
  }

  function onDoubleClick(event: MouseEvent) {
    const hit = currentHit(event);
    if (hit.kind === "columnEdge") autofit(hit.col);
    else if (hit.kind === "rowEdge") onResize("rows", [hit.row], null);
    else if (hit.kind === "cell") onEditCell();
  }

  /** double-clicking a column's edge fits it to the widest value on screen */
  function autofit(col: number) {
    const context = canvas?.getContext("2d");
    if (!context) return;
    let widest = 0;
    for (const region of layout.regions()) {
      for (let row = region.firstRow; row <= region.lastRow; row += 1) {
        const cell = view.cache.get(view.sheet, row, col);
        if (!cell?.text) continue;
        context.font = fontOf(view.styles[cell.style], 1);
        widest = Math.max(widest, context.measureText(cell.text).width);
      }
    }
    onResize("columns", [col], widest > 0 ? columnCharacters(widest + 10) : null);
  }

  function onRightClick(event: MouseEvent) {
    event.preventDefault();
    const hit = currentHit(event);
    if (hit.kind === "cell" && !view.selection.ranges.some((r) => hit.row >= r.start.row && hit.row <= r.end.row && hit.col >= r.start.col && hit.col <= r.end.col)) {
      view.select(single({ row: hit.row, col: hit.col }));
    }
    onContextMenu(event, hit);
  }

  onMount(() => {
    const observer = new ResizeObserver(() => {
      width = scroller?.clientWidth ?? 0;
      height = scroller?.clientHeight ?? 0;
      paintSoon();
    });
    if (scroller) observer.observe(scroller);
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    const rethink = () => {
      theme = null;
      paintSoon();
    };
    scheme.addEventListener("change", rethink);
    return () => {
      observer.disconnect();
      scheme.removeEventListener("change", rethink);
      cancelAnimationFrame(frame);
    };
  });

  const editorRect = $derived(editor.active && editor.sheet === view.sheet ? layout.rectOf({ start: editor.at, end: editor.at }) : null);
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="scroller"
  bind:this={scroller}
  style:cursor
  tabindex="0"
  role="grid"
  aria-label={`Sheet ${sheetName}`}
  aria-rowcount={view.rows.count}
  aria-colcount={view.cols.count}
  onscroll={onScroll}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  ondblclick={onDoubleClick}
  oncontextmenu={onRightClick}
  onkeydown={onKeyDown}
  onpaste={onPaste}
>
  <div class="spacer" style:width="{headerWidth + view.cols.total()}px" style:height="{layout.headerHeight + view.rows.total()}px"></div>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px" aria-hidden="true"></canvas>
  {#if resizeGuide}
    <div class="guide" class:across={resizeGuide.axis === "rows"} style:left={resizeGuide.axis === "columns" ? `${resizeGuide.at + scroll.x}px` : `${scroll.x}px`} style:top={resizeGuide.axis === "rows" ? `${resizeGuide.at + scroll.y}px` : `${scroll.y}px`}></div>
  {/if}
  {#if editorRect && editor.place === "cell"}
    <CellEditorField {editor} rect={{ ...editorRect, x: editorRect.x + scroll.x, y: editorRect.y + scroll.y }} zoom={view.zoom} roomRight={width - editorRect.x} onKeyDown={onEditorKey} />
  {/if}
</div>

<style>
  .scroller {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: auto;
    outline: none;
    background: #ffffff;
    overscroll-behavior: contain;
  }

  .spacer {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
  }

  /* the canvas stays in view while the spacer behind it scrolls */
  canvas {
    position: sticky;
    top: 0;
    left: 0;
    display: block;
  }

  .guide {
    position: absolute;
    width: 1px;
    height: 100%;
    background: var(--accent);
    pointer-events: none;
  }

  .guide.across {
    width: 100%;
    height: 1px;
  }
</style>
