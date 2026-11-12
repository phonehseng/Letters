// F6 and Shift F6: from region to region of the window, as in Word,
// PowerPoint and Excel. the writer's page takes Tab for indenting and the
// slide canvas takes it to step through objects, so without a key of its
// own a keyboard user who reached the page could never get back to the
// toolbar (a "keyboard trap"). the regions are the toolbars, the main area,
// the side panels and the status bar, in the order they are in the window.

/** what counts as a region. a region inside another isn't one of its own,
 * except a toolbar, which always is (the writer's sits inside its main area). */
const REGION = '[role="toolbar"], main, aside, footer';

/** what can take the keyboard inside a region. */
const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), [contenteditable="true"]';

/** in the main area, the document itself rather than the first button near it. */
const DOCUMENT = '.ProseMirror, .cm-content, [tabindex="0"]';

function shown(element: Element): boolean {
  return element.getClientRects().length > 0;
}

/** the window's regions, in order. */
export function regionsIn(root: ParentNode = document): HTMLElement[] {
  const all = [...root.querySelectorAll<HTMLElement>(REGION)].filter(shown);
  const isToolbar = (element: HTMLElement) => element.getAttribute("role") === "toolbar";
  return all.filter((region) => isToolbar(region) || !all.some((other) => other !== region && !isToolbar(other) && other.contains(region)));
}

/** where the keyboard goes in a region. */
export function focusTarget(region: HTMLElement): HTMLElement {
  const isMain = region.matches("main");
  const preferred = isMain ? region.querySelector<HTMLElement>(DOCUMENT) : null;
  if (preferred && shown(preferred)) return preferred;
  const first = [...region.querySelectorAll<HTMLElement>(FOCUSABLE)].find(shown);
  if (first) return first;
  // a region with nothing to press still takes the keyboard, so it is read out
  if (!region.hasAttribute("tabindex")) region.tabIndex = -1;
  return region;
}

/**
 * moves the keyboard to the next region (`step` 1) or the one before (-1).
 * false when the window has no regions.
 */
export function moveToRegion(step: 1 | -1, root: ParentNode = document, focused: Element | null = document.activeElement): boolean {
  const regions = regionsIn(root);
  if (regions.length === 0) return false;
  // the innermost region holding the keyboard: a toolbar inside the main area, not the area
  let current = -1;
  regions.forEach((region, index) => {
    if (focused !== null && region.contains(focused)) current = index;
  });
  const next = current < 0 ? (step === 1 ? 0 : regions.length - 1) : (current + step + regions.length) % regions.length;
  focusTarget(regions[next]).focus();
  return true;
}
