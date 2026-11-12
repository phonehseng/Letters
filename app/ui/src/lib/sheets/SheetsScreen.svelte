<script lang="ts">
  // the spreadsheet screen: one workbook per window. the grid is the
  // content; the toolbar, the formula bar, the status bar with the sheet
  // tabs, tips and Explain float over the desk as on the other screens.
  // the shell holds the workbook (lib/sheets/sheetsApi.ts); this screen
  // holds what is on show and sends every change as the engine's edits.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onDestroy, onMount, untrack } from "svelte";
  import { appCommands, keyPressOf, type Command } from "../commands";
  import ContextMenu, { type ContextItem } from "../ContextMenu.svelte";
  import type { PullDownItem } from "../writer/PullDown.svelte";
  import { writerGuard, writerMayLeave } from "../writer/writerGuard";
  import { CellEditor } from "./cellEditor.svelte";
  import type { CopiedRange } from "./clipboard";
  import FilterMenu from "./FilterMenu.svelte";
  import FormulaBar from "./FormulaBar.svelte";
  import { cellName } from "./geometry";
  import { copySelection, goTo, pasteData, startEditing, finishEditing, fillByDrag, type SheetsContext } from "./gridActions";
  import type { Hit } from "./gridHit";
  import { gridKeyAction, startsTyping } from "./gridKeys";
  import { editorKey, runGridAction } from "./screenKeys";
  import { clampRange, single } from "./selection";
  import { addSheet, applyFilter, clearFilter, deleteLines, deleteSheet, duplicateSheet, filterTarget, insertLines, moveSheet, renameSheet, showSheet, sortBy } from "./sheetActions";
  import SheetGrid from "./SheetGrid.svelte";
  import { formatItems, freezeItems, lineItems, sortItems } from "./sheetsMenus";
  import SheetsExplain from "./SheetsExplain.svelte";
  import SheetsFind from "./SheetsFind.svelte";
  import SheetsStatusBar from "./SheetsStatusBar.svelte";
  import SheetsToolbar from "./SheetsToolbar.svelte";
  import SheetTabs from "./SheetTabs.svelte";
  import { SheetsTips } from "./sheetsTips.svelte";
  import { AUTOSAVE_INTERVAL_MS, SheetsSession } from "./sheetsSession.svelte";
  import { SheetView } from "./sheetView.svelte";
  import TipCard from "./TipCard.svelte";
  import * as api from "./sheetsApi";
  import { asCommandError } from "../api";

  // long enough to read a sentence of about fifteen words
  const SPOKEN_MS = 5000;
  // a paste from the menu waits this long for the clipboard before suggesting ctrl v
  const CLIPBOARD_WAIT_MS = 1500;

  interface Props {
    opened: api.OpenedSheet;
    onError: (message: string) => void;
    onRecents: () => void;
    onNewWindow: () => void;
    /** the recovered copy opened: the window shows it instead */
    onReopen: (next: api.OpenedSheet) => void;
  }

  let { opened, onError, onRecents, onNewWindow, onReopen }: Props = $props();

  const view = untrack(() => new SheetView(opened.id, opened.activeSheet));
  const session = untrack(() => new SheetsSession(opened, onError, (outcome) => void afterChange(outcome)));
  const editor = new CellEditor();
  const tips = new SheetsTips(session);
  let grid = $state<SheetGrid>();
  let contents = $state("");
  let stats = $state<api.SelectionStats | null>(null);
  let copied = $state<CopiedRange | null>(null);
  let explainOpen = $state(false);
  let explained = $state<{ steps: api.ExplainStep[]; error: api.ErrorHelp | null }>({ steps: [], error: null });
  let shortcuts = $state<api.Shortcut[]>([]);
  let formats = $state<api.FriendlyFormat[]>([]);
  let presets = $state<string[]>([]);
  let finding = $state<{ replace: boolean } | null>(null);
  let filterMenu = $state<{ range: api.Range; column: number; x: number; y: number } | null>(null);
  let menu = $state<{ x: number; y: number; items: ContextItem[]; label: string } | null>(null);
  let spoken = $state("");
  let recovery = $state(untrack(() => opened.recovery));
  let windowWidth = $state(window.innerWidth);

  const tabs = $derived(session.state.sheets);
  const sheetName = $derived(tabs[session.activeSheet]?.name ?? "");
  const mode = $derived(editor.active ? (editor.pointing ? "Point" : editor.mode === "edit" ? "Edit" : "Enter") : "Ready");
  const formatName = $derived({ xlsx: "Excel Workbook", xlsm: "Excel Workbook with Macros", ods: "OpenDocument Spreadsheet", csv: "CSV" }[session.format]);

  const context: SheetsContext = {
    session,
    view,
    editor,
    sheetName: () => sheetName,
    screenRows: () => grid?.screenRows() ?? 20,
    screenCols: () => grid?.screenCols() ?? 8,
    reveal: (cell) => grid?.reveal(cell),
    say: (text) => (spoken = text),
    recordTyped: (at) => tips.typed(at),
  };

  async function afterChange(outcome: api.EditOutcome) {
    copied = null;
    tips.other();
    await view.afterChange(outcome);
    await refreshActive();
  }

  // --- the active cell: the formula bar, the status bar, Explain and tips

  async function refreshActive() {
    const at = view.selection.active;
    const sheet = view.sheet;
    contents = await api.editText(session.id, sheet, at.row, at.col).catch(() => "");
    const ranges = view.selection.ranges.map((range) => clampRange(range, view.usedRows(), view.usedCols()));
    stats = await api.selectionStats(session.id, sheet, ranges).catch(() => null);
    if (explainOpen) await refreshExplained();
    tips.settle(sheet, view.selection.ranges[view.selection.ranges.length - 1], editor.active);
  }

  async function refreshExplained() {
    const at = view.selection.active;
    const cell = view.cache.get(view.sheet, at.row, at.col);
    const steps = contents.startsWith("=") ? await api.explainFormula(session.id, view.sheet, at, contents).catch(() => []) : [];
    const error = cell?.text.startsWith("#") || session.state.circular.length > 0 ? await api.diagnoseCell(session.id, view.sheet, at).catch(() => null) : null;
    explained = { steps, error };
  }

  $effect(() => {
    void [view.selection, session.activeSheet, explainOpen];
    untrack(() => void refreshActive());
  });

  // --- keys

  function onGridKey(event: KeyboardEvent) {
    if (event.target !== event.currentTarget || editor.active) return;
    const press = keyPressOf(event);
    const action = gridKeyAction(press);
    if (action) {
      event.preventDefault();
      event.stopPropagation();
      if (action.kind === "escape") copied = null;
      runGridAction(context, action, hooks);
    } else if (startsTyping(press)) {
      event.preventDefault();
      event.stopPropagation();
      void startEditing(context, "enter", event.key);
    }
  }

  function onEditorKey(event: KeyboardEvent) {
    if (editorKey(context, event)) {
      event.preventDefault();
      event.stopPropagation();
      if (!editor.active) grid?.focus();
    }
  }

  const hooks = {
    copy: (cut: boolean) => void copySelection(context, cut).then((made) => (copied = made ?? copied)),
    find: (replace: boolean) => (finding = { replace }),
    filter: () => void openFilter(),
    formatMenu: () => (spoken = "The Format menu is in the toolbar: number formats, bold, colours and cell styles."),
    askLines: (insert: boolean) => askLines(insert),
  };

  async function onPaste(event: ClipboardEvent) {
    if (editor.active || !event.clipboardData) return;
    event.preventDefault();
    const result = await pasteData(context, event.clipboardData, copied);
    if (result === "moved") copied = null;
  }

  // --- menus and panels

  async function openFilter(col?: number) {
    const target = await filterTarget(context);
    const column = col !== undefined ? col - target.range.start.col : target.column;
    const box = document.querySelector(".scroller")?.getBoundingClientRect();
    filterMenu = { range: target.range, column, x: (box?.left ?? 0) + 60, y: (box?.top ?? 0) + 30 };
  }

  function askLines(insert: boolean) {
    const box = document.querySelector(".scroller")?.getBoundingClientRect();
    const run = (axis: "rows" | "columns") => (insert ? insertLines(context, axis) : deleteLines(context, axis));
    menu = {
      x: (box?.left ?? 0) + 80,
      y: (box?.top ?? 0) + 40,
      label: insert ? "Insert" : "Delete",
      items: [
        { label: insert ? "Insert Entire Rows" : "Delete Entire Rows", run: () => run("rows") },
        { label: insert ? "Insert Entire Columns" : "Delete Entire Columns", run: () => run("columns") },
      ],
    };
  }

  function onGridMenu(event: MouseEvent, hit: Hit) {
    const lines = hit.kind === "rowHeader" ? "rows" : hit.kind === "columnHeader" ? "columns" : null;
    const items: ContextItem[] = [
      { label: "Cut", run: () => hooks.copy(true) },
      { label: "Copy", run: () => hooks.copy(false) },
      { label: "Paste", title: "Ctrl V pastes from other apps too", run: () => void pasteFromMenu() },
      { label: "Clear Contents", separatorBefore: true, run: () => runGridAction(context, { kind: "clear" }, hooks) },
    ];
    if (lines) items.push({ label: `Insert ${lines === "rows" ? "Rows" : "Columns"}`, separatorBefore: true, run: () => insertLines(context, lines) }, { label: `Delete ${lines === "rows" ? "Rows" : "Columns"}`, run: () => deleteLines(context, lines) });
    else items.push({ label: "Sort A to Z", separatorBefore: true, run: () => void sortBy(context, false) }, { label: "Sort Z to A", run: () => void sortBy(context, true) }, { label: "Filter…", run: () => void openFilter() });
    items.push({ label: "Explain This Cell", separatorBefore: true, run: () => (explainOpen = true) });
    menu = { x: event.clientX, y: event.clientY, items, label: "Cell" };
  }

  async function pasteFromMenu() {
    try {
      // reading the clipboard can wait on the system's permission; ctrl v never does
      const text = await Promise.race([navigator.clipboard.readText(), new Promise<string>((_, fail) => setTimeout(() => fail(new Error("no answer")), CLIPBOARD_WAIT_MS))]);
      const transfer = new DataTransfer();
      transfer.setData("text/plain", text);
      await pasteData(context, transfer, copied);
    } catch {
      spoken = "Use Ctrl V to paste here.";
    }
  }

  function goToFound(found: api.Found) {
    const show = found.sheet !== session.activeSheet ? showSheet(context, found.sheet) : Promise.resolve();
    void show.then(() => goTo(context, { row: found.row, col: found.col }, false));
  }

  async function replaceOne(found: api.Found, find: string, replacement: string, options: api.FindOptions) {
    const text = await api.editText(session.id, found.sheet, found.row, found.col);
    const flags = options.matchCase ? "g" : "gi";
    const escaped = find.replace(/[.+^${}()|[\]\\]/g, "\\$&").replaceAll("*", ".*").replaceAll("?", ".");
    const input = options.wholeCell ? replacement : text.replace(new RegExp(escaped, flags), replacement);
    void session.apply([api.book({ op: "setCell", sheet: found.sheet, row: found.row, col: found.col, input })], "Replace");
  }

  function replaceAll(find: string, replacement: string, options: api.FindOptions, sheet: number | null) {
    void session.apply([api.book({ op: "replace", sheet, find, with: replacement, options })], "Replace All");
  }

  function applyFix(fix: api.ErrorFix) {
    void session.apply([api.book(fix.operation)], fix.title);
  }

  function goToCircular(name: string) {
    const [sheet, cell] = name.includes("!") ? name.split("!") : [sheetName, name];
    const index = tabs.findIndex((tab) => tab.name === sheet);
    const match = /^([A-Z]+)(\d+)$/.exec(cell);
    if (index < 0 || !match) return;
    let col = 0;
    for (const letter of match[1]) col = col * 26 + letter.charCodeAt(0) - 64;
    void showSheet(context, index).then(() => goTo(context, { row: Number(match[2]) - 1, col: col - 1 }, false));
    explainOpen = true;
  }

  /** after a menu command the keys go back to the grid, as after excel's ribbon */
  function backToGrid(items: PullDownItem[]): PullDownItem[] {
    return items.map((item) => ({ ...item, run: () => (item.run(), grid?.focus()) }));
  }

  // --- the app's commands while this screen shows

  $effect(() => {
    const handlers: [Command, () => void][] = [
      ["save", () => void session.save()],
      ["save-as", () => void session.saveAs()],
      ["undo", () => (editor.active ? undefined : void session.undo().then((done) => done && session.state.label && (spoken = `Undo ${session.state.label}.`)))],
      ["redo", () => (editor.active ? undefined : void session.redo().then((done) => done && session.state.label && (spoken = `Redo ${session.state.label}.`)))],
      ["find", () => (finding = { replace: false })],
      ["zoom-in", () => view.zoomStep(1)],
      ["zoom-out", () => view.zoomStep(-1)],
      ["fit-width", () => view.setZoom(1)],
    ];
    const takeBack = handlers.map(([command, handler]) => appCommands.register(command, handler));
    return () => takeBack.forEach((unregister) => unregister());
  });

  $effect(() => {
    getCurrentWindow().setTitle(session.state.dirty ? `${session.name} — Edited` : session.name);
  });

  onMount(() => {
    writerGuard.mayLeave = (ask) => writerMayLeave({ name: session.name, dirty: session.state.dirty }, ask, () => session.save(), () => session.discardRecovery());
    const timer = setInterval(() => void session.autosave(), AUTOSAVE_INTERVAL_MS);
    void (async () => {
      await view.load(session.activeSheet);
      view.select(single(view.info?.activeCell ?? { row: 0, col: 0 }));
      grid?.focus();
      [shortcuts, formats, presets] = await Promise.all([api.shortcutTable(), api.friendlyFormats(), api.stylePresets()]);
    })();
    const notes = [...opened.notes];
    if (opened.unknownFunctions.length > 0) notes.push(`This workbook uses ${opened.unknownFunctions.join(", ")}, which Letters can’t calculate yet. Those cells keep the values the file had.`);
    if (notes.length > 0) session.notice = { text: notes.join(" ") };
    // read by the end-to-end checks (nothing in the app reads it)
    (window as unknown as { __lettersSheets?: unknown }).__lettersSheets = { view, session, editor, tips, grid: () => grid, context };
    return () => clearInterval(timer);
  });

  onDestroy(() => {
    tips.stop();
    writerGuard.mayLeave = null;
    void api.closeWorkbook(session.id);
    getCurrentWindow().setTitle("Recents");
  });

  async function recover() {
    recovery = null;
    if (!session.path) return;
    try {
      onReopen(await api.openWorkbook(session.path, true));
    } catch (problem) {
      onError(asCommandError(problem).message);
    }
  }

  // a spoken message shows for a moment, then goes
  $effect(() => {
    if (!spoken) return;
    const timer = setTimeout(() => (spoken = ""), SPOKEN_MS);
    return () => clearTimeout(timer);
  });</script>

<svelte:window bind:innerWidth={windowWidth} />

<div class="sheets-screen">
  <div class="main">
    <SheetsToolbar
      name={session.name}
      edited={session.state.dirty}
      {formatName}
      narrow={windowWidth < 900}
      {explainOpen}
      formatItems={backToGrid(formatItems(context, formats, presets))}
      sortItems={backToGrid(sortItems(context, () => void openFilter()))}
      lineItems={backToGrid(lineItems(context))}
      freezeItems={backToGrid(freezeItems(context))}
      {onRecents}
      onSave={() => void session.save()}
      onFind={() => (finding = { replace: false })}
      {onNewWindow}
      onExplain={() => (explainOpen = !explainOpen)}
    />

    {#if recovery}
      <div class="glass notice" role="alertdialog" aria-label="Recovered version">
        <span>Letters kept unsaved changes to this workbook from {new Date(recovery.savedAt * 1000).toLocaleString()}, when it last closed without saving.</span>
        <button class="button" onclick={() => ((recovery = null), void session.discardRecovery())}>Discard</button>
        <button class="button primary" onclick={recover}>Recover</button>
      </div>
    {:else if session.notice}
      <div class="glass notice" role="status">
        <span>{session.notice.text}</span>
        {#if session.notice.action}
          <button class="button" onclick={() => { const action = session.notice?.action; session.notice = null; action?.run(); }}>{session.notice.action.label}</button>
        {/if}
        <button class="button" onclick={() => (session.notice = null)}>OK</button>
      </div>
    {/if}

    <FormulaBar
      {editor}
      selection={view.selection}
      {contents}
      onGoTo={(range) => { view.select({ ranges: [range], active: range.start, anchor: range.start }); grid?.reveal(range.start); grid?.focus(); }}
      onStartEditing={() => void startEditing(context, "edit").then(() => (editor.place = "bar"))}
      onKeyDown={onEditorKey}
      onExplain={() => (explainOpen = true)}
      onNameBoxError={(message) => (spoken = message)}
    />

    <div class="paper">
      <SheetGrid
        bind:this={grid}
        {view}
        {editor}
        {sheetName}
        copied={copied && copied.sheet === view.sheet ? copied.range : null}
        onKeyDown={onGridKey}
        {onPaste}
        onEditCell={() => void startEditing(context, "edit")}
        onFill={(source, target) => fillByDrag(context, source, target)}
        onResize={(axis, indices, size) => void session.resize(axis, indices, size)}
        onFilterButton={(col) => void openFilter(col)}
        onContextMenu={onGridMenu}
        {onEditorKey}
        onCommitEdit={() => void finishEditing(context, null)}
      />
    </div>

    <SheetsStatusBar {stats} circular={session.state.circular} zoom={view.zoom} {mode} onCircular={goToCircular} onZoom={() => view.setZoom(1)}>
      {#snippet tabs()}
        <SheetTabs
          tabs={session.state.sheets}
          active={session.activeSheet}
          onShow={(index) => void showSheet(context, index)}
          onAdd={() => addSheet(context)}
          onRename={(index, name) => renameSheet(context, index, name)}
          onMove={(from, to) => moveSheet(context, from, to)}
          onDuplicate={(index) => duplicateSheet(context, index)}
          onDelete={(index) => deleteSheet(context, index)}
        />
      {/snippet}
    </SheetsStatusBar>

    {#if tips.shown}
      <TipCard tip={tips.shown} onApply={() => void tips.apply()} onNotNow={() => tips.notNow()} onNever={() => tips.never()} />
    {/if}
    {#if finding}
      <SheetsFind id={session.id} sheet={session.activeSheet} sheetNames={tabs.map((tab) => tab.name)} bind:showReplace={finding.replace} onGo={goToFound} onReplaceOne={(f, a, b, o) => void replaceOne(f, a, b, o)} onReplaceAll={replaceAll} onClose={() => ((finding = null), grid?.focus())} />
    {/if}
    <p class="visually-hidden" aria-live="polite">{spoken}</p>
    {#if spoken}<div class="glass spoken" role="presentation">{spoken}<button class="button" onclick={() => (spoken = "")}>OK</button></div>{/if}
  </div>

  {#if explainOpen}
    <SheetsExplain cellName={cellName(view.selection.active.row, view.selection.active.col)} steps={explained.steps} error={explained.error} {shortcuts} onFix={applyFix} onClose={() => (explainOpen = false)} />
  {/if}
</div>

{#if filterMenu}
  <FilterMenu
    id={session.id}
    sheet={view.sheet}
    range={filterMenu.range}
    column={filterMenu.column}
    x={filterMenu.x}
    y={filterMenu.y}
    filtered={!!view.info?.filter}
    onApply={(keep) => { if (filterMenu) applyFilter(context, filterMenu.range, filterMenu.column, keep); filterMenu = null; }}
    onSort={(descending) => { filterMenu = null; void sortBy(context, descending); }}
    onClear={() => { filterMenu = null; clearFilter(context); }}
    onClose={() => (filterMenu = null)}
  />
{/if}
{#if menu}
  <ContextMenu x={menu.x} y={menu.y} label={menu.label} items={menu.items} onClose={() => (menu = null)} />
{/if}

<style>
  .sheets-screen {
    display: flex;
    height: 100%;
    background: var(--desk);
  }

  .main {
    position: relative;
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  /* the sheet is the content: it fills the desk between the floating bars */
  .paper {
    display: flex;
    flex: 1;
    min-height: 0;
    margin: 0 0.5rem;
    overflow: hidden;
    border-radius: var(--radius-group);
    box-shadow: var(--shadow-page);
  }

  .notice,
  .spoken {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    margin: 0 auto 0.5rem;
    max-width: min(40rem, calc(100% - 2rem));
    padding: 0.625rem 0.625rem 0.625rem 1rem;
    border-radius: var(--radius-group);
  }

  .notice span {
    flex: 1 1 14rem;
  }

  .spoken {
    position: absolute;
    bottom: 4rem;
    left: 50%;
    z-index: 4;
    margin: 0;
    transform: translateX(-50%);
    font: var(--body);
  }
</style>
