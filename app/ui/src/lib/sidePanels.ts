// the panels beside the document: the sidebar on the leading side, and
// the Tools inspector and the Explain panel on the trailing side.
//
// the rule: the document always keeps a readable width. opening a panel
// that doesn't fit closes the panel opened longest ago, until it does.
// on a wide window all three can be open; on a small one they take turns.
// below a certain width the inspector stops taking room from the document
// and floats over it instead, like a sheet, so the toolbar never has to
// squeeze into a strip.

// "ai" is the AI Tools panel (and Find a Topic), on the trailing side too
export type SidePanel = "sidebar" | "inspector" | "explain" | "ai";

/** how wide each panel is, in rem (the components use the same numbers). */
export const PANEL_WIDTH_REM: Record<SidePanel, number> = { sidebar: 12, inspector: 18, explain: 17.5, ai: 19 };

/** the document area never gets narrower than this because of panels, in rem. */
export const DOCUMENT_MIN_REM = 26;

/** below this window width, in rem, the inspector floats over the document. */
export const INSPECTOR_FLOATS_BELOW_REM = 46;

export function inspectorFloats(windowWidthPx: number, remPx: number): boolean {
  return windowWidthPx < INSPECTOR_FLOATS_BELOW_REM * remPx;
}

/** how much width the open panels take from the document, in rem. */
function usedRem(open: SidePanel[], floating: boolean): number {
  return open
    .filter((panel) => !(floating && panel === "inspector"))
    .reduce((sum, panel) => sum + PANEL_WIDTH_REM[panel], 0);
}

/**
 * keeps the newest panels that fit, dropping the oldest first. `open` is
 * in the order the panels were opened, oldest first.
 */
export function fitPanels(open: SidePanel[], windowWidthPx: number, remPx: number): SidePanel[] {
  const floating = inspectorFloats(windowWidthPx, remPx);
  const windowRem = windowWidthPx / remPx;
  const kept = [...open];
  // a floating inspector covers the document, so nothing else shows beside it
  if (floating && kept.includes("inspector")) {
    return kept[kept.length - 1] === "inspector" ? ["inspector"] : kept.filter((panel) => panel !== "inspector").slice(-1);
  }
  while (kept.length > 1 && windowRem - usedRem(kept, floating) < DOCUMENT_MIN_REM) kept.shift();
  return kept;
}

/** opens a panel (it becomes the newest) and closes what no longer fits. */
export function openPanel(open: SidePanel[], panel: SidePanel, windowWidthPx: number, remPx: number): SidePanel[] {
  return fitPanels([...open.filter((other) => other !== panel), panel], windowWidthPx, remPx);
}

export function closePanel(open: SidePanel[], panel: SidePanel): SidePanel[] {
  return open.filter((other) => other !== panel);
}

export function togglePanel(open: SidePanel[], panel: SidePanel, windowWidthPx: number, remPx: number): SidePanel[] {
  return open.includes(panel) ? closePanel(open, panel) : openPanel(open, panel, windowWidthPx, remPx);
}
