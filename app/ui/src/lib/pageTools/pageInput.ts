// the pointer on the pages, handed to the active page tool.
//
// used as a svelte action on the scroll content that holds the pages:
//   <div use:pageInput={{ tool: () => tool, frames: () => frames }}>
// a press on a page becomes a PagePointer (page points, the click
// count, the keys held) for the tool's `down`; the rest of the press goes
// to the gesture the tool returned. the pointer is captured, so a drag
// keeps going even off the window. coalesced events are used while
// moving, so a quick pen stroke keeps every point the system saw.
//
// controls floating over the pages (a selected object's frame, a text
// box being typed, a note's pop-up, the selection bar) mark themselves
// with `data-page-ui` and keep their own presses.

import { countClick, type ClickRecord } from "./gestures";
import { contentPointOf, frameAt, pagePointOf, type PageFrame } from "./pagePoints";
import type { Gesture, PagePointer, PageTool } from "./pageTool";

export interface PageInputOptions {
  tool: () => PageTool;
  frames: () => PageFrame[];
  /** a press has started (true) or ended (false) */
  onPress?: (pressing: boolean) => void;
  /** the document should take the keyboard focus, for shift with the arrows and copy */
  focus?: () => void;
}

interface Press {
  id: number;
  gesture: Gesture;
  /** where it started; a drawing tool keeps drawing on this page */
  frame: PageFrame;
  crossesPages: boolean;
  clicks: number;
}

export function pageInput(node: HTMLElement, initial: PageInputOptions) {
  let options = initial;
  let press: Press | null = null;
  let lastClick: ClickRecord | null = null;

  function pointerAt(event: PointerEvent, frame: PageFrame, clicks: number): PagePointer {
    const point = contentPointOf(event.clientX, event.clientY, node.getBoundingClientRect());
    const onPage = pagePointOf(frame, point);
    return {
      ...onPage,
      scale: frame.scale,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      clicks,
      time: event.timeStamp,
    };
  }

  /** during a drag: the page under the pointer (or nearest), or the page it started on. */
  function frameDuring(event: PointerEvent, current: Press): PageFrame {
    if (!current.crossesPages) return current.frame;
    const point = contentPointOf(event.clientX, event.clientY, node.getBoundingClientRect());
    return frameAt(options.frames(), point, true) ?? current.frame;
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0 || press) return;
    const target = event.target as Element | null;
    if (target?.closest("[data-page-ui]")) return;
    const point = contentPointOf(event.clientX, event.clientY, node.getBoundingClientRect());
    const frame = frameAt(options.frames(), point);
    if (!frame) return;
    const onPage = pagePointOf(frame, point);
    lastClick = countClick(lastClick, event.timeStamp, frame.index, onPage);
    const tool = options.tool();
    const gesture = tool.down(pointerAt(event, frame, lastClick.count));
    options.focus?.();
    if (!gesture) return;
    // the page's own text selection and image dragging would fight the tool
    event.preventDefault();
    node.setPointerCapture(event.pointerId);
    press = { id: event.pointerId, gesture, frame, crossesPages: tool.crossesPages, clicks: lastClick.count };
    options.onPress?.(true);
  }

  function onPointerMove(event: PointerEvent) {
    if (!press || event.pointerId !== press.id) return;
    const current = press;
    // the points the system gathered since the last event, for smooth strokes
    const events = typeof event.getCoalescedEvents === "function" ? event.getCoalescedEvents() : [];
    for (const each of events.length > 0 ? events : [event]) {
      current.gesture.move(pointerAt(each, frameDuring(each, current), current.clicks));
    }
  }

  function onPointerUp(event: PointerEvent) {
    if (!press || event.pointerId !== press.id) return;
    const current = press;
    press = null;
    current.gesture.up(pointerAt(event, frameDuring(event, current), current.clicks));
    options.onPress?.(false);
  }

  function onPointerCancel(event: PointerEvent) {
    if (!press || event.pointerId !== press.id) return;
    const current = press;
    press = null;
    current.gesture.cancel();
    options.onPress?.(false);
  }

  /** escape during a drag: the gesture is dropped. true when there was one. */
  function cancelPress(): boolean {
    if (!press) return false;
    const current = press;
    press = null;
    current.gesture.cancel();
    options.onPress?.(false);
    return true;
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "Escape" && cancelPress()) {
      event.preventDefault();
      event.stopPropagation();
    }
  }

  node.addEventListener("pointerdown", onPointerDown);
  node.addEventListener("pointermove", onPointerMove);
  node.addEventListener("pointerup", onPointerUp);
  node.addEventListener("pointercancel", onPointerCancel);
  window.addEventListener("keydown", onKeyDown, true);

  return {
    update(next: PageInputOptions) {
      options = next;
    },
    destroy() {
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerCancel);
      window.removeEventListener("keydown", onKeyDown, true);
    },
  };
}
