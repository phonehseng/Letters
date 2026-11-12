<script lang="ts">
  // the spreadsheet screen's floating toolbar, in the three places
  // DESIGN.md gives every toolbar: leading (back to Recents, the
  // workbook's name, Save), centre (Format, Sort & Filter, Rows &
  // Columns, Freeze: menus, so the toolbar stays short) and trailing
  // (Find, New Window, Explain). the menus' items come from SheetsScreen.
  import ToolButton from "../ToolButton.svelte";
  import { tools, tooltipFor } from "../tools";
  import PullDown, { type PullDownItem } from "../writer/PullDown.svelte";

  interface Props {
    name: string;
    edited: boolean;
    formatName: string;
    narrow: boolean;
    explainOpen: boolean;
    formatItems: PullDownItem[];
    sortItems: PullDownItem[];
    lineItems: PullDownItem[];
    freezeItems: PullDownItem[];
    onRecents: () => void;
    onSave: () => void;
    onFind: () => void;
    onNewWindow: () => void;
    onExplain: () => void;
  }

  let props: Props = $props();
</script>

<div class="toolbar" role="toolbar" aria-label="Spreadsheet tools">
  <div class="group leading glass">
    <ToolButton name="recents" icon="back" symbolOnly onClick={props.onRecents} />
    <div class="title">
      <strong>{props.name}{props.edited ? " — Edited" : ""}</strong>
      <span>{props.formatName}</span>
    </div>
    <button class="button save" onclick={props.onSave} disabled={!props.edited} title={tooltipFor("sheetsSave")}>{tools.sheetsSave.label}</button>
  </div>

  <div class="group glass">
    <PullDown label={tools.sheetsFormat.label} tooltip={tooltipFor("sheetsFormat")} icon="format" items={props.formatItems} hideLabel={props.narrow} />
    <PullDown label={tools.sheetsSortFilter.label} tooltip={tooltipFor("sheetsSortFilter")} icon="down" items={props.sortItems} hideLabel={props.narrow} />
    <PullDown label={tools.sheetsRowsColumns.label} tooltip={tooltipFor("sheetsRowsColumns")} icon="table" items={props.lineItems} hideLabel={props.narrow} />
    <PullDown label={tools.sheetsFreeze.label} tooltip={tooltipFor("sheetsFreeze")} icon="ruler" items={props.freezeItems} hideLabel={props.narrow} />
  </div>

  <div class="group glass">
    <ToolButton name="sheetsFind" icon="search" hideLabel={props.narrow} onClick={props.onFind} />
    <ToolButton name="newWindow" icon="newWindow" hideLabel={props.narrow} onClick={props.onNewWindow} />
    <ToolButton name="explain" icon="sidePanel" hideLabel={props.narrow} pressed={props.explainOpen} onClick={props.onExplain} />
  </div>
</div>

<style>
  /* above the formula bar, so the menus open over it */
  .toolbar {
    position: relative;
    z-index: 10;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 0.125rem;
    padding: var(--group-padding);
    border-radius: var(--radius-group);
  }

  .leading {
    flex: 1;
    min-width: 0;
  }

  .title {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 0 0.5rem;
  }

  .title strong {
    overflow: hidden;
    font: var(--headline);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title span {
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  .save {
    margin-left: auto;
  }
</style>
