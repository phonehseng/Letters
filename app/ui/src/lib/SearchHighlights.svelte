<script lang="ts">
  // the find highlights of one page, drawn over it.
  //
  // this layer sits beside the page instead of inside it, so night mode's
  // colour flip (applied to the page) does not turn yellow into blue.
  import type { Highlight } from "./search";

  interface Props {
    highlights: Highlight[];
    left: number;
    top: number;
    width: number;
    height: number;
  }

  let { highlights, left, top, width, height }: Props = $props();
</script>

<div
  class="highlights"
  aria-hidden="true"
  style:left="{left}px"
  style:top="{top}px"
  style:width="{width}px"
  style:height="{height}px"
>
  {#each highlights as highlight (highlight.key)}
    <span
      class:current={highlight.current}
      style:left="{highlight.left}px"
      style:top="{highlight.top}px"
      style:width="{highlight.width}px"
      style:height="{highlight.height}px"
    ></span>
  {/each}
</div>

<style>
  .highlights {
    position: absolute;
    /* clicks go through to the page underneath */
    pointer-events: none;
  }

  /* highlighter yellow, see-through so the words stay readable */
  span {
    position: absolute;
    border-radius: 2px;
    background: rgb(255 214 10 / 0.38);
  }

  /* the current match is stronger and slightly larger, so the eye finds it */
  span.current {
    background: rgb(255 159 10 / 0.55);
    box-shadow: 0 0 0 2px rgb(255 159 10 / 0.55);
  }

  @media (prefers-contrast: more) {
    span {
      background: rgb(255 204 0 / 0.5);
    }

    span.current {
      box-shadow: 0 0 0 2px rgb(200 90 0);
    }
  }
</style>
