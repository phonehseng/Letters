<script lang="ts">
  // the marks for redaction on one page: outlined, hatched boxes, so a
  // mark can never be mistaken for a finished redaction (which the engine
  // draws as a solid box in the page itself). a click picks a mark; the
  // picked one gets the shared frame, so it can be moved, resized and
  // removed with the pointer or the keyboard. the viewer puts one of
  // these over every page on screen.
  import ObjectFrame from "../pageTools/ObjectFrame.svelte";
  import { HANDLES } from "../pageTools/objectFrame";
  import type { PageFrame } from "../pageTools/pagePoints";
  import { markLabel, marksOn } from "./pendingMarks";
  import type { ProtectSession } from "./protectSession.svelte";

  interface Props {
    session: ProtectSession;
    frame: PageFrame;
    /** gives the keyboard back to the document, after Escape or Delete */
    onFocusDocument: () => void;
  }

  let { session, frame, onFocusDocument }: Props = $props();

  const marks = $derived(marksOn(session.marks, frame.index));
  const selected = $derived(session.selectedMark?.page === frame.index ? session.selectedMark : null);
  const hatch = $derived(`redaction-hatch-${frame.index}`);

  function frameLabel(): string {
    if (!selected) return "";
    return `Area marked for redaction on page ${frame.index + 1}: ${markLabel(selected)}. Not removed yet. The arrow keys move it, with Shift for 10 points. Alt with the arrows changes its size. Delete removes the mark. Escape lets go.`;
  }

  function pick(event: PointerEvent, id: string) {
    if (event.button !== 0) return;
    event.stopPropagation();
    session.selectMark(id, true);
  }
</script>

{#if marks.length > 0}
  <svg
    class="redaction-marks"
    aria-hidden="true"
    viewBox="0 0 {frame.size.width} {frame.size.height}"
    style:left="{frame.left}px"
    style:top="{frame.top}px"
    style:width="{frame.width}px"
    style:height="{frame.height}px"
  >
    <defs>
      <pattern id={hatch} width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
        <line x1="0" y1="0" x2="0" y2="6" class="hatch-line" />
      </pattern>
    </defs>
    {#each marks as mark (mark.id)}
      {@const rect = mark.id === selected?.id && session.framePreview ? session.framePreview : mark.rect}
      <rect class="mark" x={rect.x} y={rect.y} width={rect.width} height={rect.height} fill="url(#{hatch})" vector-effect="non-scaling-stroke" />
    {/each}
  </svg>
  {#each marks as mark (mark.id)}
    {#if mark.id !== selected?.id}
      <!-- the keyboard reaches marks through the list in the Protect tab -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="hit"
        data-page-ui
        title="Marked for redaction: {markLabel(mark)}"
        style:left="{frame.left + mark.rect.x * frame.scale}px"
        style:top="{frame.top + mark.rect.y * frame.scale}px"
        style:width="{mark.rect.width * frame.scale}px"
        style:height="{mark.rect.height * frame.scale}px"
        onpointerdown={(event) => pick(event, mark.id)}
      ></div>
    {/if}
  {/each}
{/if}

{#if selected}
  {#key `${selected.id}-${session.focusTick}`}
    <ObjectFrame
      {frame}
      rect={session.framePreview ?? selected.rect}
      label={frameLabel()}
      movable={true}
      handles={HANDLES}
      minSize={2}
      focusOnShow={true}
      name="redaction-mark"
      onPreview={(rect) => (session.framePreview = rect)}
      onCommit={(rect) => session.moveSelectedMark(rect)}
      onDelete={() => {
        session.removeMark(selected.id);
        onFocusDocument();
      }}
      onEscape={() => {
        session.deselectMark();
        onFocusDocument();
      }}
    />
  {/key}
{/if}

<style>
  .redaction-marks {
    position: absolute;
    overflow: visible;
    pointer-events: none;
  }

  /* red, not the accent: the accent means selected, and these say "to be removed" */
  .mark {
    stroke: rgb(215 0 21);
    stroke-width: 1.5;
    stroke-dasharray: 5 3;
  }

  .hatch-line {
    stroke: rgb(215 0 21 / 0.45);
    stroke-width: 1.5;
  }

  .hit {
    position: absolute;
    cursor: pointer;
  }

  @media (prefers-contrast: more) {
    .mark {
      stroke-width: 2.5;
      stroke-dasharray: none;
    }

    .hatch-line {
      stroke: rgb(215 0 21 / 0.7);
    }
  }
</style>
