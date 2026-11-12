// snap guides while an object is dragged: a line-for-line port of the
// office engine's floating::snap (core/office/src/floating.rs), with the
// same test case. it runs on every pointer move, where a round trip to the
// shell would cost frames, so it lives here like typing.ts does; the
// result is the same function of the same numbers.
//
// everything is in page coordinates: twips from the page's top-left.

export interface PageBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SnapGuide {
  /** true for a vertical line (an x position) */
  vertical: boolean;
  position: number;
  kind: "page edge" | "margin" | "page centre" | "object edge" | "object centre";
}

export interface SnapPage {
  width: number;
  height: number;
  margins: { top: number; right: number; bottom: number; left: number };
}

type Line = [number, SnapGuide["kind"]];

/** snaps `proposed` to page edges, margins, the page's centre lines and other objects within `threshold` twips. */
export function snap(page: SnapPage, others: PageBox[], proposed: PageBox, threshold: number): { rect: PageBox; guides: SnapGuide[] } {
  const verticalLines: Line[] = [
    [0, "page edge"],
    [page.width, "page edge"],
    [page.margins.left, "margin"],
    [page.width - page.margins.right, "margin"],
    [Math.trunc(page.width / 2), "page centre"],
  ];
  const horizontalLines: Line[] = [
    [0, "page edge"],
    [page.height, "page edge"],
    [page.margins.top, "margin"],
    [page.height - page.margins.bottom, "margin"],
    [Math.trunc(page.height / 2), "page centre"],
  ];
  for (const r of others) {
    verticalLines.push([r.x, "object edge"], [r.x + r.width, "object edge"], [r.x + Math.trunc(r.width / 2), "object centre"]);
    horizontalLines.push([r.y, "object edge"], [r.y + r.height, "object edge"], [r.y + Math.trunc(r.height / 2), "object centre"]);
  }
  const rect = { ...proposed };
  const guides: SnapGuide[] = [];
  // the box's left edge, centre and right edge, each against every line
  const half = (size: number) => Math.trunc(size / 2);
  const bestX = best(
    [
      [0, rect.x],
      [half(rect.width), rect.x + half(rect.width)],
      [rect.width, rect.x + rect.width],
    ],
    verticalLines,
    threshold,
  );
  if (bestX) {
    rect.x = bestX.line - bestX.shift;
    guides.push({ vertical: true, position: bestX.line, kind: bestX.kind });
  }
  const bestY = best(
    [
      [0, rect.y],
      [half(rect.height), rect.y + half(rect.height)],
      [rect.height, rect.y + rect.height],
    ],
    horizontalLines,
    threshold,
  );
  if (bestY) {
    rect.y = bestY.line - bestY.shift;
    guides.push({ vertical: false, position: bestY.line, kind: bestY.kind });
  }
  return { rect, guides };
}

/** the closest pairing of a box position and a line within the threshold (the first one wins a tie). */
function best(candidates: [number, number][], lines: Line[], threshold: number): { shift: number; line: number; kind: SnapGuide["kind"] } | null {
  let found: { distance: number; shift: number; line: number; kind: SnapGuide["kind"] } | null = null;
  for (const [shift, position] of candidates) {
    for (const [line, kind] of lines) {
      const distance = Math.abs(position - line);
      if (distance <= threshold && (!found || distance < found.distance)) found = { distance, shift, line, kind };
    }
  }
  return found ? { shift: found.shift, line: found.line, kind: found.kind } : null;
}

/** how far snapping reaches: about six screen pixels at the current zoom. */
export function snapThreshold(zoom: number): number {
  return Math.round((6 / Math.max(zoom, 0.1)) * 15);
}
