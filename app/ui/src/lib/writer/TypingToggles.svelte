<script lang="ts">
  // the four typing switches: Autocorrect, Capitalize, Smart Quotes and
  // Spelling. the same component sits in the status bar (compact) and in
  // the Typing section of the Document tab, and both change the one set
  // of settings stored for this person.
  import { tools, tooltipFor, type ToolName } from "../tools";
  import { setToggle, toggleIsOn, type TypingToggle } from "./typing";
  import type { WriterSettings } from "./writerApi";

  interface Props {
    settings: WriterSettings;
    compact?: boolean;
    onChange: (settings: WriterSettings) => void;
  }

  let { settings, compact = false, onChange }: Props = $props();

  const TOGGLES: { key: TypingToggle; tool: ToolName }[] = [
    { key: "autocorrect", tool: "wAutocorrect" },
    { key: "capitalize", tool: "wCapitalize" },
    { key: "smartQuotes", tool: "wSmartQuotes" },
    { key: "spellcheck", tool: "wSpellcheck" },
  ];

  function isOn(key: TypingToggle): boolean {
    return key === "spellcheck" ? settings.spellcheck : toggleIsOn(settings.typing, key);
  }

  function flip(key: TypingToggle) {
    const on = !isOn(key);
    if (key === "spellcheck") onChange({ ...settings, spellcheck: on });
    else onChange({ ...settings, typing: setToggle(settings.typing, key, on) });
  }
</script>

{#if compact}
  <div class="compact" role="group" aria-label="Typing options">
    {#each TOGGLES as toggle (toggle.key)}
      <button class="pill" class:on={isOn(toggle.key)} aria-pressed={isOn(toggle.key)} title={tooltipFor(toggle.tool)} onclick={() => flip(toggle.key)}>
        {tools[toggle.tool].label}
      </button>
    {/each}
  </div>
{:else}
  <div class="list" role="group" aria-label="Typing options">
    {#each TOGGLES as toggle (toggle.key)}
      <label class="row" title={tooltipFor(toggle.tool)}>
        <span>
          <span class="name">{tools[toggle.tool].label}</span>
          <span class="hint">{tools[toggle.tool].tooltip}.</span>
        </span>
        <input type="checkbox" role="switch" checked={isOn(toggle.key)} onchange={() => flip(toggle.key)} />
      </label>
    {/each}
  </div>
{/if}

<style>
  .compact {
    display: flex;
    gap: 0.25rem;
  }

  /* 24px tall at least (WCAG's target size); an off switch keeps the
     secondary colour, since the line through it already says it is off */
  .pill {
    min-height: 1.5rem;
    padding: 0.125rem 0.5rem;
    border: 0;
    border-radius: 0.625rem;
    background: transparent;
    color: var(--label-secondary);
    font: var(--footnote);
    letter-spacing: var(--footnote-spacing);
    text-decoration: line-through;
    transition: background 0.15s var(--ease);
  }

  .pill.on {
    color: var(--label-secondary);
    text-decoration: none;
  }

  .pill:hover {
    background: var(--control-hover);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .name {
    display: block;
    font: var(--body);
    font-weight: 500;
  }

  .hint {
    display: block;
    color: var(--label-secondary);
    font: var(--subheadline);
  }

  input {
    flex-shrink: 0;
    accent-color: var(--accent);
  }
</style>
