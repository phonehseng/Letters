<script lang="ts">
  // the writer's floating toolbar, in the viewer's three areas: where you
  // are (sidebar, Recents, the document's name and menu), what the text
  // is (the style menu and zoom), and what else you can do (Insert, Find,
  // View, a new window, the Format inspector and Explain). formatting
  // beyond the style lives in the inspector, not in a ribbon.
  import ToolButton from "../ToolButton.svelte";
  import { tools, tooltipFor } from "../tools";
  import PullDown, { type PullDownItem } from "./PullDown.svelte";
  import TableGrid from "./TableGrid.svelte";
  import WriterButton from "./WriterButton.svelte";
  import { shortcutLabel } from "./shortcuts";
  import { documentStyleChoices, STANDARD_STYLES } from "./styles";
  import type { WriterCommand } from "./writerCommands";

  interface Props {
    name: string;
    edited: boolean;
    canUndo: boolean;
    canRedo: boolean;
    zoom: number;
    narrow: boolean;
    styleKey: string;
    styles: import("./model").StyleSheet;
    sidebarOpen: boolean;
    findOpen: boolean;
    inspectorOpen: boolean;
    explainOpen: boolean;
    nightMode: boolean;
    rulerShown: boolean;
    height?: number;
    onCommand: (command: WriterCommand | "save" | "save-as" | "undo" | "redo") => void;
    onStyle: (key: string) => void;
    onInsertTable: (rows: number, columns: number) => void;
    onSidebar: () => void;
    onRecents: () => void;
    onZoomOut: () => void;
    onZoomIn: () => void;
    onFitWidth: () => void;
    onFind: () => void;
    onNightMode: () => void;
    onRuler: () => void;
    onNewWindow: () => void;
    onInspector: () => void;
    onExplain: () => void;
  }

  let {
    name,
    edited,
    canUndo,
    canRedo,
    zoom,
    narrow,
    styleKey,
    styles,
    sidebarOpen,
    findOpen,
    inspectorOpen,
    explainOpen,
    nightMode,
    rulerShown,
    height = $bindable(0),
    onCommand,
    onStyle,
    onInsertTable,
    onSidebar,
    onRecents,
    onZoomOut,
    onZoomIn,
    onFitWidth,
    onFind,
    onNightMode,
    onRuler,
    onNewWindow,
    onInspector,
    onExplain,
  }: Props = $props();

  const documentItems = $derived<PullDownItem[]>([
    { label: tools.wSave.label, shortcut: "Ctrl S", disabled: !edited, title: edited ? tools.wSave.tooltip : tools.wSave.unavailable, run: () => onCommand("save") },
    { label: tools.wSaveAs.label, shortcut: "Ctrl Shift S", title: tools.wSaveAs.tooltip, run: () => onCommand("save-as") },
    { label: tools.wExportPdf.label, title: tools.wExportPdf.tooltip, separatorBefore: true, run: () => onCommand("export-pdf") },
    { label: tools.wSaveToDrive.label, title: tools.wSaveToDrive.tooltip, run: () => onCommand("save-to-drive") },
    { label: tools.wPrint.label, shortcut: "Ctrl P", title: tools.wPrint.tooltip, run: () => onCommand("print") },
    { label: tools.wOpenPdfAsDocument.label, title: tools.wOpenPdfAsDocument.unavailable, disabled: true, run: () => {} },
    { label: tools.undo.label, shortcut: "Ctrl Z", disabled: !canUndo, separatorBefore: true, run: () => onCommand("undo") },
    { label: tools.redo.label, shortcut: "Ctrl Shift Z", disabled: !canRedo, run: () => onCommand("redo") },
  ]);

  const styleItems = $derived<PullDownItem[]>([
    ...STANDARD_STYLES.map((choice) => ({ label: choice.label, shortcut: choice.shortcut, checked: styleKey === choice.key, run: () => onStyle(choice.key) })),
    ...documentStyleChoices(styles).map((choice, index) => ({
      label: choice.label,
      checked: styleKey === choice.key,
      separatorBefore: index === 0,
      run: () => onStyle(choice.key),
    })),
  ]);

  const currentStyleLabel = $derived(
    STANDARD_STYLES.find((choice) => choice.key === styleKey)?.label ?? documentStyleChoices(styles).find((choice) => choice.key === styleKey)?.label ?? "Style",
  );

  const insertItems = $derived<PullDownItem[]>([
    { label: tools.wInsertPicture.label, title: tools.wInsertPicture.tooltip, run: () => onCommand("insert-picture") },
    { label: tools.wInsertPageBreak.label, shortcut: shortcutLabel("insert-page-break"), run: () => onCommand("insert-page-break") },
    { label: tools.wInsertFootnote.label, shortcut: shortcutLabel("insert-footnote"), run: () => onCommand("insert-footnote") },
    { label: tools.wInsertLink.label, shortcut: shortcutLabel("insert-link"), run: () => onCommand("insert-link") },
    { label: tools.wInsertPageNumber.label, title: tools.wInsertPageNumber.tooltip, run: () => onCommand("insert-page-number") },
    { label: tools.wInsertComment.label, shortcut: shortcutLabel("insert-comment"), run: () => onCommand("insert-comment") },
  ]);

  const viewItems = $derived<PullDownItem[]>([
    { label: tools.wRuler.label, checked: rulerShown, title: tools.wRuler.tooltip, run: onRuler },
    { label: tools.nightMode.label, checked: nightMode, title: tools.nightMode.tooltip, run: onNightMode },
  ]);
</script>

<div class="toolbar" role="toolbar" aria-label="Document tools" bind:clientHeight={height}>
  <div class="area leading">
    <div class="glass group">
      <ToolButton name="wOutline" icon="sidebar" hideLabel={narrow} pressed={sidebarOpen} onClick={onSidebar} />
      <ToolButton name="recents" icon="back" symbolOnly onClick={onRecents} />
      <PullDown label="Document" tooltip={tooltipFor("documentMenu")} icon="document" items={documentItems} title={{ name, edited }} menuLabel="Document" />
    </div>
  </div>

  <div class="area centre">
    <div class="glass group">
      <PullDown label={currentStyleLabel} tooltip={tooltipFor("wStyle")} icon="style" items={styleItems} hideLabel={false} menuLabel="Paragraph styles" />
      {#if !narrow}
        <ToolButton name="zoomOut" icon="zoomOut" hideLabel onClick={onZoomOut} />
        <button class="zoom-level" title={tooltipFor("fitWidth")} onclick={onFitWidth}>{Math.round(zoom * 100)}%</button>
        <ToolButton name="zoomIn" icon="zoomIn" hideLabel onClick={onZoomIn} />
      {/if}
    </div>
  </div>

  <div class="area trailing">
    <div class="glass group">
      <PullDown label={tools.wInsert.label} tooltip={tooltipFor("wInsert")} icon="insert" items={insertItems} hideLabel={narrow} menuLabel="Insert">
        {#snippet extra(close)}
          <TableGrid
            onPick={(rows, columns) => {
              close();
              onInsertTable(rows, columns);
            }}
          />
        {/snippet}
      </PullDown>
      <ToolButton name="find" icon="search" hideLabel={narrow} pressed={findOpen} onClick={onFind} />
      <PullDown label={tools.view.label} tooltip={tooltipFor("view")} icon="ruler" items={viewItems} hideLabel={narrow} />
      {#if !narrow}
        <ToolButton name="newWindow" icon="newWindow" onClick={onNewWindow} />
      {/if}
      <WriterButton name="wFormat" icon="format" hideLabel={narrow} pressed={inspectorOpen} onClick={onInspector} />
      <ToolButton name="explain" icon="sidePanel" hideLabel={narrow} pressed={explainOpen} onClick={onExplain} />
    </div>
  </div>
</div>

<style>
  .toolbar {
    position: absolute;
    inset: 0 0.75rem auto 0;
    /* above notices, so a menu is never covered by one */
    z-index: 5;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: start;
    gap: 0.75rem;
    padding: 0.625rem 0 0.625rem 0.75rem;
    pointer-events: none;
  }

  .toolbar::before {
    content: "";
    position: absolute;
    inset: 0 0 -0.75rem 0;
    background: linear-gradient(to bottom, var(--desk) 35%, transparent);
    opacity: 0.9;
  }

  .area {
    position: relative;
    display: flex;
    min-width: 0;
  }

  .area.trailing {
    justify-content: flex-end;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 0.125rem;
    min-width: 0;
    padding: var(--group-padding);
    pointer-events: auto;
  }

  .zoom-level {
    min-width: 2.75rem;
    height: 2rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-variant-numeric: tabular-nums;
  }

  .zoom-level:hover {
    background: var(--control-hover);
  }
</style>
