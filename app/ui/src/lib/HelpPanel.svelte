<script lang="ts">
  // the "what's this?" side panel: every tool on the current screen,
  // with when to use it and what it does to the file.
  import Icon from "./Icon.svelte";
  import { tools, type ToolName } from "./tools";

  interface Props {
    names: ToolName[];
    onClose: () => void;
  }

  let { names, onClose }: Props = $props();
</script>

<aside class="help" aria-label="What each tool does">
  <header>
    <h2>What each tool does</h2>
    <button class="close" title="Close this panel" aria-label="Close this panel" onclick={onClose}>
      <Icon name="close" size={0.9} />
    </button>
  </header>
  <dl>
    {#each names as name (name)}
      {@const tool = tools[name]}
      <div class="entry">
        <dt>{tool.label}</dt>
        <dd class="summary">{tool.tooltip}.</dd>
        <dd><span>When to use it</span>{tool.whenToUse}</dd>
        <dd><span>What happens to your file</span>{tool.fileEffect}</dd>
      </div>
    {/each}
  </dl>
</aside>

<style>
  .help {
    width: 18.5rem;
    max-width: 80vw;
    height: 100%;
    overflow-y: auto;
    padding: 1rem 1.1rem;
    border-left: 0.5px solid var(--hairline);
    background: var(--bar);
    backdrop-filter: blur(24px) saturate(1.8);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font-size: 0.8125rem;
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
    color: var(--text-secondary);
  }

  .close:hover {
    background: var(--control-hover);
    color: var(--text);
  }

  dl {
    margin: 0.75rem 0 0;
  }

  .entry {
    padding: 0.8rem 0;
    border-top: 0.5px solid var(--hairline);
  }

  dt {
    font-size: 0.875rem;
    font-weight: 600;
  }

  dd {
    margin: 0.45rem 0 0;
    font-size: 0.8125rem;
  }

  dd.summary {
    margin-top: 0.1rem;
    color: var(--text-secondary);
  }

  /* the small grey caption above each answer */
  dd span {
    display: block;
    color: var(--text-secondary);
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
</style>
