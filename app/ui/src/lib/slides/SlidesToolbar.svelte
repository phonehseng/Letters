<script lang="ts">
  // the slide editor's floating toolbar, in the three places DESIGN.md
  // gives every toolbar: leading (the slide rail, back to Recents, the
  // presentation's name as its menu), centre (New Slide and zoom) and
  // trailing (Insert, Play, Find, Notes, a new window, Format, Explain).
  // the words come from slidesTools.ts.
  import ToolButton from "../ToolButton.svelte";
  import { tools, tooltipFor } from "../tools";
  import PullDown, { type PullDownItem } from "../writer/PullDown.svelte";
  import TableGrid from "../writer/TableGrid.svelte";
  import { isMac, shortcutText } from "./slidesCommands";

  export type ToolbarAction =
    | "save"
    | "save-as"
    | "export-pdf"
    | "undo"
    | "redo"
    | "new-slide"
    | "play"
    | "play-here"
    | "presenter"
    | "text-box"
    | "picture"
    | `shape:${string}`;

  interface Props {
    name: string;
    edited: boolean;
    canUndo: boolean;
    canRedo: boolean;
    zoom: number;
    narrow: boolean;
    railOpen: boolean;
    findOpen: boolean;
    notesOpen: boolean;
    inspectorOpen: boolean;
    explainOpen: boolean;
    onAction: (action: ToolbarAction) => void;
    onInsertTable: (rows: number, columns: number) => void;
    onRail: () => void;
    onRecents: () => void;
    onZoomOut: () => void;
    onZoomIn: () => void;
    onFit: () => void;
    onFind: () => void;
    onNotes: () => void;
    onNewWindow: () => void;
    onInspector: () => void;
    onExplain: () => void;
  }

  let props: Props = $props();
  const mac = isMac();
  // 100 % is the slide at its real size: a point is 96 / 72 of a css pixel
  const POINTS_TO_PIXELS = 72 / 96;
  const key = (windows: string, macKeys: string) => (mac ? macKeys : windows);

  /** the shapes Insert offers, by their drawingml preset names */
  const SHAPES: [string, string][] = [
    ["rect", "Rectangle"],
    ["roundRect", "Rounded Rectangle"],
    ["ellipse", "Oval"],
    ["triangle", "Triangle"],
    ["rightArrow", "Arrow"],
    ["star5", "Star"],
  ];

  const documentItems = $derived<PullDownItem[]>([
    { label: tools.sSave.label, shortcut: key("Ctrl S", "⌘S"), disabled: !props.edited, title: props.edited ? tools.sSave.tooltip : tools.sSave.unavailable, run: () => props.onAction("save") },
    { label: tools.sSaveAs.label, shortcut: key("Ctrl Shift S", "⇧⌘S"), title: tools.sSaveAs.tooltip, run: () => props.onAction("save-as") },
    { label: tools.sExportPdf.label, title: tools.sExportPdf.tooltip, separatorBefore: true, run: () => props.onAction("export-pdf") },
    { label: "Play from This Slide", shortcut: shortcutText("slides-play-from-here", mac), separatorBefore: true, run: () => props.onAction("play-here") },
    { label: tools.sPresenterView.label, shortcut: shortcutText("slides-presenter-view", mac), title: tools.sPresenterView.tooltip, run: () => props.onAction("presenter") },
    { label: tools.undo.label, shortcut: key("Ctrl Z", "⌘Z"), disabled: !props.canUndo, separatorBefore: true, run: () => props.onAction("undo") },
    { label: tools.redo.label, shortcut: key("Ctrl Shift Z", "⇧⌘Z"), disabled: !props.canRedo, run: () => props.onAction("redo") },
  ]);

  const insertItems = $derived<PullDownItem[]>([
    { label: tools.sTextBox.label, title: tools.sTextBox.tooltip, run: () => props.onAction("text-box") },
    { label: tools.sPicture.label, title: tools.sPicture.tooltip, run: () => props.onAction("picture") },
    ...SHAPES.map(([preset, label], index) => ({ label, title: tools.sShape.tooltip, separatorBefore: index === 0, run: () => props.onAction(`shape:${preset}`) })),
  ]);
</script>

<div class="toolbar" role="toolbar" aria-label="Presentation tools">
  <div class="area leading">
    <div class="glass group">
      <ToolButton name="sSlides" icon="sidebar" hideLabel={props.narrow} pressed={props.railOpen} onClick={props.onRail} />
      <ToolButton name="recents" icon="back" symbolOnly onClick={props.onRecents} />
      <PullDown label="Presentation" tooltip={tooltipFor("sDocument")} icon="document" items={documentItems} title={{ name: props.name, edited: props.edited }} menuLabel="Presentation" />
    </div>
  </div>

  <div class="area centre">
    <div class="glass group">
      <ToolButton name="sNewSlide" icon="plus" hideLabel={props.narrow} onClick={() => props.onAction("new-slide")} />
      {#if !props.narrow}
        <ToolButton name="zoomOut" icon="zoomOut" hideLabel onClick={props.onZoomOut} />
        <button class="zoom-level" title="Fit the slide in the window" onclick={props.onFit}>{Math.round(props.zoom * POINTS_TO_PIXELS * 100)}%</button>
        <ToolButton name="zoomIn" icon="zoomIn" hideLabel onClick={props.onZoomIn} />
      {/if}
    </div>
  </div>

  <div class="area trailing">
    <div class="glass group">
      <PullDown label={tools.sInsert.label} tooltip={tooltipFor("sInsert")} icon="insert" items={insertItems} hideLabel={props.narrow} menuLabel="Insert">
        {#snippet extra(close)}
          <TableGrid
            onPick={(rows, columns) => {
              close();
              props.onInsertTable(rows, columns);
            }}
          />
        {/snippet}
      </PullDown>
      <ToolButton name="sPlay" icon="chevronRight" hideLabel={props.narrow} onClick={() => props.onAction("play")} />
      <ToolButton name="sFind" icon="search" hideLabel={props.narrow} pressed={props.findOpen} onClick={props.onFind} />
      <ToolButton name="sNotes" icon="arrowDown" hideLabel={props.narrow} pressed={props.notesOpen} onClick={props.onNotes} />
      {#if !props.narrow}
        <ToolButton name="newWindow" icon="newWindow" onClick={props.onNewWindow} />
      {/if}
      <ToolButton name="sFormat" icon="tools" hideLabel={props.narrow} pressed={props.inspectorOpen} onClick={props.onInspector} />
      <ToolButton name="explain" icon="sidePanel" hideLabel={props.narrow} pressed={props.explainOpen} onClick={props.onExplain} />
    </div>
  </div>
</div>

<style>
  .toolbar {
    position: relative;
    z-index: 5;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: start;
    gap: 0.75rem;
    padding: 0.625rem 0.75rem;
  }

  .area {
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
