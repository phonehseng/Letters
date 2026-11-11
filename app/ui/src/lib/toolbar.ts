// what the viewer's toolbar gives up as the window gets narrow.
//
// the toolbar must never overflow, down to the 480 px minimum window
// (and less, when a side panel takes part of it). things give way in a
// fixed order: first the words under the symbols (each button keeps its
// name for screen readers and its tooltip), then the zoom percentage,
// then the least important buttons, the last added going first. every
// hidden button has a keyboard shortcut, and the explain panel lists it.

export type ToolbarPart =
  | "labels"
  | "zoomLevel"
  | "newWindow"
  | "pageWord"
  | "fitWidth"
  | "zoomButtons"
  | "find";

/** each part hides when the document area is narrower than this, in rem. */
export const NARROWING: readonly { part: ToolbarPart; belowRem: number }[] = [
  { part: "labels", belowRem: 54 },
  // without the percentage the zoom group stops being centred, so the
  // title gets the room instead
  { part: "zoomLevel", belowRem: 44 },
  // ctrl n
  { part: "newWindow", belowRem: 36 },
  // "page 3 of 40" becomes "3 of 40"
  { part: "pageWord", belowRem: 36 },
  // ctrl 0
  { part: "fitWidth", belowRem: 32 },
  // ctrl + and ctrl −
  { part: "zoomButtons", belowRem: 26 },
  // ctrl f. only reached with a side panel open on a small window
  { part: "find", belowRem: 20 },
];

/**
 * the parts hidden at a width. a width of 0 means "not measured yet",
 * and then nothing is hidden, so the toolbar does not flicker on load.
 */
export function hiddenParts(areaWidthPx: number, remPx: number): Set<ToolbarPart> {
  if (areaWidthPx <= 0 || remPx <= 0) return new Set();
  const widthRem = areaWidthPx / remPx;
  return new Set(NARROWING.filter((rule) => widthRem < rule.belowRem).map((rule) => rule.part));
}
