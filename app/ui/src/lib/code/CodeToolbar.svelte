<script lang="ts">
  // the code screen's floating toolbar, in the three places DESIGN.md
  // gives every toolbar: leading (back to Recents, the file's name),
  // centre (Find, Go to Line, Comment) and trailing (Problems, VS Code,
  // Show in Folder, New Window, Explain). words come from codeTools.ts.
  import ToolButton from "../ToolButton.svelte";
  import { tools } from "../tools";

  interface Props {
    name: string;
    edited: boolean;
    languageLabel: string;
    errorCount: number;
    problemsOpen: boolean;
    explainOpen: boolean;
    canSave: boolean;
    /** false when vs code isn't installed: the button is left out */
    vscode: boolean;
    /** false for an untitled block, which has no folder yet */
    hasFile: boolean;
    narrow: boolean;
    onRecents: () => void;
    onSave: () => void;
    onFind: () => void;
    onGoToLine: () => void;
    onComment: () => void;
    onProblems: () => void;
    onVsCode: () => void;
    onReveal: () => void;
    onNewWindow: () => void;
    onExplain: () => void;
  }

  let props: Props = $props();
</script>

<div class="toolbar">
  <div class="group leading glass">
    <ToolButton name="recents" icon="back" symbolOnly onClick={props.onRecents} />
    <div class="title">
      <strong>{props.name}{props.edited ? " — Edited" : ""}</strong>
      <span>{props.languageLabel}</span>
    </div>
    {#if props.canSave}
      <button class="button save" onclick={props.onSave} disabled={!props.edited} title={tools.codeSave.tooltip}>{tools.codeSave.label}</button>
    {/if}
  </div>

  <div class="group glass">
    <ToolButton name="codeFind" icon="search" hideLabel={props.narrow} onClick={props.onFind} />
    <ToolButton name="codeGoToLine" icon="arrowDown" hideLabel={props.narrow} onClick={props.onGoToLine} />
    <ToolButton name="codeToggleComment" icon="chevronRight" hideLabel={props.narrow} onClick={props.onComment} />
  </div>

  <div class="group glass">
    <ToolButton name="codeProblems" icon="check" hideLabel={props.narrow} pressed={props.problemsOpen} onClick={props.onProblems} />
    {#if props.errorCount > 0}
      <span class="badge" aria-hidden="true">{props.errorCount}</span>
    {/if}
    {#if props.vscode && props.hasFile}
      <ToolButton name="codeOpenInVsCode" icon="open" hideLabel={props.narrow} onClick={props.onVsCode} />
    {/if}
    {#if props.hasFile && !props.narrow}
      <ToolButton name="codeReveal" icon="pinFolder" onClick={props.onReveal} />
    {/if}
    <ToolButton name="newWindow" icon="newWindow" hideLabel={props.narrow} onClick={props.onNewWindow} />
    <ToolButton name="explain" icon="sidePanel" hideLabel={props.narrow} pressed={props.explainOpen} onClick={props.onExplain} />
  </div>
</div>

<style>
  .toolbar {
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

  .badge {
    min-width: 1rem;
    margin-left: -1.25rem;
    padding: 0 0.25rem;
    border-radius: 0.5rem;
    background: var(--danger);
    color: #ffffff;
    font: var(--footnote);
    text-align: center;
    align-self: flex-start;
  }
</style>
