<script lang="ts">
  // what the editor draws over the slide that the slide itself doesn't
  // show: the dashed outline and grey words of an empty placeholder
  // ("Click to add title", as powerpoint shows them while editing, never
  // in the show or in exports), a faint outline under the pointer, and a
  // quiet chip on things kept as they are (a chart, SmartArt, an embedded
  // object) with the sentence that explains it.
  import type { ObjectView } from "./slidesApi";

  interface Props {
    objects: ObjectView[];
    scale: number;
    hovered: number | null;
    /** the object being typed into, which shows its own field instead */
    editing: number | null;
  }

  let { objects, scale, hovered, editing }: Props = $props();

  const KEPT_WORDS: Record<string, string> = { chart: "Chart", diagram: "SmartArt", embedded: "Embedded object" };

  function place(object: ObjectView) {
    return `left:${object.x * scale}px;top:${object.y * scale}px;width:${object.width * scale}px;height:${object.height * scale}px;transform:rotate(${object.rotation}deg)`;
  }
</script>

{#each objects as object (object.id)}
  {#if object.id !== editing}
    {#if object.text?.prompt}
      {@const first = object.text.paragraphs[0]}
      <div class="mark prompt {object.text.anchor}" style={place(object)} aria-hidden="true">
        <span
          style:font-family="'{first.font}', sans-serif"
          style:font-size="{first.size * object.text.fontScale * scale}px"
          style:text-align={first.align}
          style:padding="0 {(object.width - object.text.area[2]) * scale}px 0 {object.text.area[0] * scale}px">{object.text.prompt}</span
        >
      </div>
    {:else if object.id === hovered}
      <div class="mark hover" style={place(object)} aria-hidden="true"></div>
    {/if}
    {#if KEPT_WORDS[object.kind]}
      <div class="mark kept-box" style={place(object)}>
        <span class="chip" title={object.kept ?? ""}>{KEPT_WORDS[object.kind]} — kept as it is</span>
      </div>
    {/if}
  {/if}
{/each}

<style>
  .mark {
    position: absolute;
    box-sizing: border-box;
    pointer-events: none;
  }

  .prompt {
    display: flex;
    flex-direction: column;
    border: 1px dashed rgb(0 0 0 / 0.35);
  }

  .prompt.top {
    justify-content: flex-start;
  }

  .prompt.middle {
    justify-content: center;
  }

  .prompt.bottom {
    justify-content: flex-end;
  }

  .prompt span {
    display: block;
    color: rgb(0 0 0 / 0.45);
    line-height: 1.2;
  }

  .hover {
    border: 1px dashed rgb(0 0 0 / 0.3);
  }

  .kept-box {
    border: 1px dashed rgb(0 0 0 / 0.3);
  }

  .chip {
    position: absolute;
    left: 0.25rem;
    top: 0.25rem;
    padding: 0.125rem 0.375rem;
    border-radius: var(--radius-small);
    background: var(--glass);
    box-shadow: var(--shadow-glass);
    color: var(--label-secondary);
    font: var(--footnote);
    pointer-events: auto;
  }
</style>
