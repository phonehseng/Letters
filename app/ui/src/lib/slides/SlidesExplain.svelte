<script lang="ts">
  // Explain for the slide editor: every tool on the screen with when to
  // use it and what it does to the file, the selected object's own
  // sentence when the file keeps something about it Letters doesn't draw,
  // and the slide show's keys.
  import { tools, type ToolHelp } from "../tools";
  import type { ObjectView } from "./slidesApi";
  import { slidesTools } from "./slidesTools";

  interface Props {
    selected: ObjectView[];
    onClose: () => void;
  }

  let { selected, onClose }: Props = $props();

  const names = Object.keys(slidesTools) as (keyof typeof slidesTools)[];
  const entries = names.map((name) => ({ name, help: tools[name] as ToolHelp }));
  const kept = $derived(selected.filter((object) => object.kept));
</script>

<aside class="glass explain" aria-label="Explain">
  <header>
    <h2>Explain</h2>
    <button class="button" onclick={onClose}>Close</button>
  </header>
  {#each kept as object (object.id)}
    <section class="selected">
      <h3>{object.name}</h3>
      <p>{object.kept}</p>
    </section>
  {/each}
  {#each entries as entry (entry.name)}
    <section>
      <h3>{entry.help.label}{entry.help.shortcut ? ` (${entry.help.shortcut})` : ""}</h3>
      {#if entry.help.meaning}<p>{entry.help.meaning}</p>{/if}
      <p>{entry.help.whenToUse}</p>
      <p class="effect"><strong>The file:</strong> {entry.help.fileEffect}</p>
    </section>
  {/each}
  <section>
    <h3>Slide Show Keys</h3>
    <p>→, ↓, Space, Enter or N: next. ←, ↑, Backspace or P: back. A number then Enter: that slide. B or .: black screen; W or ,: white screen. Home and End: first and last. Esc: end the show.</p>
  </section>
</aside>

<style>
  .explain {
    width: 20rem;
    max-width: 50vw;
    margin: 0 0.5rem 0.5rem 0;
    padding: 0.75rem 1rem;
    overflow: auto;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  h3 {
    margin: 0.875rem 0 0.25rem;
    font: var(--headline);
  }

  p {
    margin: 0.25rem 0;
    font: var(--body);
  }

  .effect {
    color: var(--label-secondary);
  }

  .selected {
    padding: 0.25rem 0.5rem 0.5rem;
    border-radius: var(--radius-small);
    background: var(--accent-soft);
  }
</style>
