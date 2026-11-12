<script lang="ts">
  // Explain for the writer: what the ruler says about the paragraph with
  // the cursor (the office engine's own sentences), then every tool on
  // screen with when to use it and what it does to the file, the open
  // inspector tab's tools first.
  import Icon from "../Icon.svelte";
  import { tools, type ToolHelp, type ToolName } from "../tools";

  interface Props {
    groups: { title: string; names: ToolName[] }[];
    ruler: string[];
    onClose: () => void;
  }

  let { groups, ruler, onClose }: Props = $props();
  const shown = $derived(groups.map((group) => ({ title: group.title, entries: group.names.map((name): ToolHelp => tools[name]) })));
</script>

<aside class="panel" aria-label="What each tool does">
  <header>
    <h2>Explain</h2>
    <button class="close" title="Close this panel" aria-label="Close" onclick={onClose}>
      <Icon name="close" size={0.75} />
    </button>
  </header>
  {#if ruler.length}
    <h3>This Paragraph on the Ruler</h3>
    <div class="ruler-words" aria-live="polite">
      {#each ruler as sentence (sentence)}<p>{sentence}</p>{/each}
    </div>
  {/if}
  {#each shown as group (group.title)}
    <h3>{group.title}</h3>
    <dl>
      {#each group.entries as tool (tool.label + tool.tooltip)}
        <div class="entry">
          <dt>
            {tool.label.replace("…", "")}
            {#if tool.shortcut}<kbd>{tool.shortcut}</kbd>{/if}
          </dt>
          <dd class="summary">{tool.tooltip.replace(/\.$/, "")}.</dd>
          {#if tool.meaning}<dd>{tool.meaning}</dd>{/if}
          <dd><span>When to use it</span>{tool.whenToUse}</dd>
          <dd><span>What happens to the file</span>{tool.fileEffect}</dd>
        </div>
      {/each}
    </dl>
  {/each}
</aside>

<style>
  .panel {
    width: min(17.5rem, 45vw);
    height: 100%;
    overflow-y: auto;
    padding: 1rem 1rem 3rem;
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

  h3 {
    margin: 0.75rem 0 0;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
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
  }

  .ruler-words p {
    margin: 0.375rem 0 0;
    line-height: 1.125rem;
  }

  dl {
    margin: 0;
  }

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
  }

  dd {
    margin: 0.5rem 0 0;
    line-height: 1.125rem;
  }

  dd.summary {
    margin-top: 0.125rem;
    color: var(--label-secondary);
  }

  dd span {
    display: block;
    margin-bottom: 0.0625rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }
</style>
