<script lang="ts">
  // the crop rectangle over the page being read, while the Crop tool is
  // open. the part that will be hidden is shaded; the rectangle moves when
  // dragged and resizes from its eight handles. from the keyboard, the
  // arrows move it and shift with the arrows changes its size (alt for
  // ten points at a time).
  //
  // it is the same frame every selected object on a page uses
  // (pageTools/ObjectFrame.svelte); only the keyboard differs. the rule
  // that nothing leaves the page is in cropRect.ts.
  import { asCropBox, asRect, keepOnPage, SMALLEST_CROP, type CropBox } from "./cropRect";
  import type { PageSize, PlacedPage } from "./layout";
  import { HANDLES } from "./pageTools/objectFrame";
  import ObjectFrame from "./pageTools/ObjectFrame.svelte";
  import { frameOf, type Rect } from "./pageTools/pagePoints";

  interface Props {
    page: PlacedPage;
    /** the page's size in points, as shown */
    size: PageSize;
    /** the page's left edge in the scroll area, css pixels */
    left: number;
    box: CropBox;
    onChange: (box: CropBox) => void;
  }

  let { page, size, left, box, onChange }: Props = $props();

  const frame = $derived(frameOf(page, size, left));

  const STEPS: Record<string, [number, number]> = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  };

  /** the crop rectangle's own keys: shift resizes, alt moves ten points at a time. */
  function cropKey(event: KeyboardEvent, rect: Rect): Rect | null {
    const step = STEPS[event.key];
    if (!step) return null;
    const amount = event.altKey ? 10 : 1;
    const [dx, dy] = [step[0] * amount, step[1] * amount];
    const current = asCropBox(rect);
    const next = event.shiftKey
      ? { ...current, width: current.width + dx, height: current.height + dy }
      : { ...current, left: current.left + dx, top: current.top + dy };
    return asRect(keepOnPage(next, size));
  }
</script>

<ObjectFrame
  {frame}
  rect={asRect(box)}
  label="Crop rectangle. Drag it or use the arrow keys to move it; Shift with the arrows changes its size."
  name="crop"
  movable
  handles={HANDLES}
  minSize={SMALLEST_CROP}
  shade
  keyCommitDelay={0}
  keyRect={cropKey}
  onPreview={(rect) => onChange(asCropBox(rect))}
  onCommit={(rect) => onChange(asCropBox(rect))}
/>
