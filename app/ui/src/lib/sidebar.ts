// whether the sidebar is open and what it shows.
//
// the sidebar keeps its last mode while closed, so the toolbar button
// brings back whatever the reader used last.

export type SidebarMode = "thumbnails" | "outline";

export interface SidebarState {
  open: boolean;
  mode: SidebarMode;
}

export const CLOSED_SIDEBAR: SidebarState = { open: false, mode: "thumbnails" };

/** the toolbar button: open or close, keeping the mode. */
export function toggleSidebar(state: SidebarState): SidebarState {
  return { ...state, open: !state.open };
}

/**
 * the "show thumbnails" and "show outline" commands. like the menu
 * items in Preview and Pages, choosing the mode already on screen
 * closes the sidebar; any other choice opens it in that mode.
 */
export function toggleSidebarMode(state: SidebarState, mode: SidebarMode): SidebarState {
  if (state.open && state.mode === mode) return { ...state, open: false };
  return { open: true, mode };
}

/**
 * below this window width (in rem) the sidebar and the explain panel
 * take turns: with both open the document would be squeezed to a strip.
 */
export const ONE_PANEL_BELOW_REM = 48;

export function onlyOnePanelFits(windowWidthPx: number, remPx: number): boolean {
  return windowWidthPx < ONE_PANEL_BELOW_REM * remPx;
}
