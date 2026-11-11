<script lang="ts">
  // a toolbar button: an icon with its name underneath.
  //
  // the label and tooltip come from tools.ts, so no button can be added
  // without a plain-english description. the name is always visible
  // because an icon alone makes people guess.
  import Icon, { type IconName } from "./Icon.svelte";
  import { tools, type ToolName } from "./tools";

  interface Props {
    name: ToolName;
    icon: IconName;
    onClick: () => void;
    /** for on/off tools such as night mode */
    pressed?: boolean;
  }

  let { name, icon, onClick, pressed }: Props = $props();
  const tool = $derived(tools[name]);
</script>

<button class="tool" class:pressed title={tool.tooltip} aria-pressed={pressed} onclick={onClick}>
  <Icon name={icon} />
  <span>{tool.label}</span>
</button>

<style>
  .tool {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.1rem;
    min-width: 3.4rem;
    padding: 0.3rem 0.5rem 0.2rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.6875rem;
    white-space: nowrap;
    transition:
      background 0.15s var(--ease),
      color 0.15s var(--ease);
  }

  .tool:hover {
    background: var(--control);
    color: var(--text);
  }

  .tool:active {
    background: var(--control-active);
  }

  .tool.pressed {
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
