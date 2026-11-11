<script lang="ts">
  // the explain panel: every tool on the current screen, with when to
  // use it and what it does to the file.
  //
  // it sits on the trailing side like an inspector, and is closed until
  // asked for, so the explanations never crowd the document.
  import Icon from "./Icon.svelte";
  import { tools, type ToolHelp, type ToolName } from "./tools";

  interface Props {
    names: ToolName[];
    onClose: () => void;
  }

  let { names, onClose }: Props = $props();
  const entries = $derived(names.map((name): ToolHelp => tools[name]));
</script>

<aside class="panel" aria-label="What each tool does">
  <header>
    <h2>Explain</h2>
    <button class="close" title="Close this panel" aria-label="Close" onclick={onClose}>
      <Icon name="close" size={0.75} />
    </button>
  </header>
  <dl>
    {#each entries as tool (tool.label)}
      <div class="entry">
        <dt>
          {tool.label.replace("…", "")}
          {#if tool.shortcut}
            <kbd>{tool.shortcut}</kbd>
          {/if}
        </dt>
        <dd class="summary">{tool.tooltip}.</dd>
        <dd><span>When to use it</span>{tool.whenToUse}</dd>
        <dd><span>What happens to the file</span>{tool.fileEffect}</dd>
      </div>
    {/each}
  </dl>
</aside>

<style>
  /* runs the full height of the window, edge to edge, like a sidebar */
  .panel {
    width: 17.5rem;
    max-width: 80vw;
    height: 100%;
    overflow-y: auto;
    padding: 1rem 1rem 1.5rem;
    border-left: 0.5px solid var(--separator);
    background: var(--glass);
    backdrop-filter: var(--blur);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 0.5rem;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .close {
    display: grid;
    place-items: center;
    width: 1.5rem;
    height: 1.5rem;
    border: 0;
    border-radius: 50%;
    background: var(--control);
    color: var(--label-secondary);
    transition: background 0.15s var(--ease);
  }

  .close:hover {
    background: var(--control-hover);
  }

  .close:active {
    background: var(--control-active);
  }

  dl {
    margin: 0;
  }

  /* entries are set apart by space, with only a hairline between them */
  .entry {
    padding: 0.875rem 0;
    border-top: 0.5px solid var(--separator);
  }

  dt {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    font: var(--headline);
  }

  kbd {
    color: var(--label-secondary);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  /* a little more line height than a label: these are sentences to read */
  dd {
    margin: 0.5rem 0 0;
    line-height: 1.125rem;
  }

  dd.summary {
    margin-top: 0.125rem;
    color: var(--label-secondary);
  }

  /* the small caption above each answer */
  dd span {
    display: block;
    margin-bottom: 0.0625rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
    letter-spacing: var(--subheadline-spacing);
  }
</style>
