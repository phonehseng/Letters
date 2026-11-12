<script lang="ts">
  // what the page tools draw over one page while someone works: the text
  // selection, and the live preview of a gesture under way (a rectangle
  // being dragged, an ink stroke, a line).
  //
  // it is an svg whose coordinates are page points, so the marks are
  // exactly where the engine will put the real thing, at any zoom. it
  // sits beside the page rather than inside it, so night mode's colour
  // flip doesn't change the colours, and clicks go through it.
  import type { PageFrame, Rect } from "./pagePoints";
  import type { Mark, MarkStyle } from "./pageTool";

  interface Props {
    frame: PageFrame;
    /** the text selection on this page, one box per line */
    selection: Rect[];
    marks: Mark[];
  }

  let { frame, selection, marks }: Props = $props();

  function strokeOf(style: MarkStyle) {
    return {
      stroke: style.stroke ?? "none",
      fill: style.fill ?? "none",
      "stroke-width": style.width,
      "stroke-dasharray": style.dash?.join(" "),
      opacity: style.opacity,
    };
  }

  /** a smooth enough path through the points of one stroke. */
  function strokePath(points: { x: number; y: number }[]): string {
    if (points.length === 0) return "";
    const [first, ...rest] = points;
    if (rest.length === 0) return `M${first.x} ${first.y}l0.01 0.01`;
    return `M${first.x} ${first.y}` + rest.map((point) => `L${point.x} ${point.y}`).join("");
  }

  /** the two sides of an open arrowhead at the end of a line. */
  function arrowHead(start: { x: number; y: number }, end: { x: number; y: number }, width: number): string {
    const angle = Math.atan2(end.y - start.y, end.x - start.x);
    const length = Math.max(8, width * 4);
    const side = (turn: number) => ({
      x: end.x - length * Math.cos(angle + turn),
      y: end.y - length * Math.sin(angle + turn),
    });
    const [a, b] = [side(Math.PI / 7), side(-Math.PI / 7)];
    return `M${a.x} ${a.y}L${end.x} ${end.y}L${b.x} ${b.y}`;
  }
</script>

{#if selection.length > 0 || marks.length > 0}
  <svg
    class="marks"
    aria-hidden="true"
    viewBox="0 0 {frame.size.width} {frame.size.height}"
    style:left="{frame.left}px"
    style:top="{frame.top}px"
    style:width="{frame.width}px"
    style:height="{frame.height}px"
  >
    {#each selection as rect, index (index)}
      <rect class="selected-text" x={rect.x} y={rect.y} width={rect.width} height={rect.height} />
    {/each}
    {#each marks as mark, index (index)}
      {#if mark.kind === "rect"}
        <rect x={mark.rect.x} y={mark.rect.y} width={mark.rect.width} height={mark.rect.height} {...strokeOf(mark.style)} />
      {:else if mark.kind === "ellipse"}
        <ellipse
          cx={mark.rect.x + mark.rect.width / 2}
          cy={mark.rect.y + mark.rect.height / 2}
          rx={mark.rect.width / 2}
          ry={mark.rect.height / 2}
          {...strokeOf(mark.style)}
        />
      {:else if mark.kind === "line"}
        <g {...strokeOf(mark.style)} stroke-linecap="round" stroke-linejoin="round" fill="none">
          <line x1={mark.start.x} y1={mark.start.y} x2={mark.end.x} y2={mark.end.y} />
          {#if mark.arrow}<path d={arrowHead(mark.start, mark.end, mark.style.width)} />{/if}
        </g>
      {:else if mark.kind === "ink"}
        <g {...strokeOf(mark.style)} fill="none" stroke-linecap="round" stroke-linejoin="round">
          {#each mark.strokes as points, strokeIndex (strokeIndex)}
            <path d={strokePath(points)} />
          {/each}
        </g>
      {:else if mark.kind === "textBoxes"}
        {#each mark.rects as rect, rectIndex (rectIndex)}
          <rect class="selected-text" x={rect.x} y={rect.y} width={rect.width} height={rect.height} />
        {/each}
      {/if}
    {/each}
  </svg>
{/if}

<style>
  .marks {
    position: absolute;
    overflow: visible;
    /* presses go through to the page, where the tool hears them */
    pointer-events: none;
  }

  /* selection is the accent's job: the same blue as selected thumbnails */
  .selected-text {
    fill: rgb(0 136 255 / 0.28);
  }

  @media (prefers-contrast: more) {
    .selected-text {
      fill: rgb(30 110 244 / 0.4);
    }
  }
</style>
