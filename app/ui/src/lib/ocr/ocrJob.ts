// the life of one Recognize Text run, as the Scan tab shows it. a pure
// state machine, so every step (including a cancel that lands while the
// last page finishes) is tested without the shell.
//
//   idle ──start──> starting ──started──> running ──status──> running ...
//                                            │  └─cancel──> cancelling ──status(cancelled)──> cancelled
//                                            └─status(finished)──> applying ──applied──> done
//   any failure ──> failed. reset goes back to idle from an ending.

import type { JobStatus, OcrSummary } from "./ocrApi";

export type JobView =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "running"; job: number; done: number; total: number; current: number | null }
  | { phase: "cancelling"; job: number; done: number; total: number }
  | { phase: "applying"; job: number }
  | { phase: "done"; summary: OcrSummary }
  | { phase: "cancelled"; done: number; total: number }
  | { phase: "failed"; message: string };

export type JobEvent =
  | { type: "start" }
  | { type: "started"; job: number; total: number }
  | { type: "status"; status: JobStatus }
  | { type: "cancel" }
  | { type: "applied"; summary: OcrSummary }
  | { type: "failed"; message: string }
  | { type: "reset" };

export const IDLE: JobView = { phase: "idle" };

/** a run is under way: the tab shows progress and Cancel, not Recognize Text. */
export function isBusy(view: JobView): boolean {
  return view.phase === "starting" || view.phase === "running" || view.phase === "cancelling" || view.phase === "applying";
}

/** the run the shell knows about, while there is one to ask after. */
export function jobOf(view: JobView): number | null {
  return view.phase === "running" || view.phase === "cancelling" || view.phase === "applying" ? view.job : null;
}

export function nextView(view: JobView, event: JobEvent): JobView {
  switch (event.type) {
    case "start":
      return isBusy(view) ? view : { phase: "starting" };
    case "started":
      return view.phase === "starting" ? { phase: "running", job: event.job, done: 0, total: event.total, current: null } : view;
    case "status":
      return afterStatus(view, event.status);
    case "cancel":
      return view.phase === "running" ? { phase: "cancelling", job: view.job, done: view.done, total: view.total } : view;
    case "applied":
      return view.phase === "applying" ? { phase: "done", summary: event.summary } : view;
    case "failed":
      return { phase: "failed", message: event.message };
    case "reset":
      return isBusy(view) ? view : IDLE;
  }
}

function afterStatus(view: JobView, status: JobStatus): JobView {
  if (view.phase !== "running" && view.phase !== "cancelling") return view;
  if (status.id !== view.job) return view;
  const { done, total, current } = status.progress;
  switch (status.state) {
    case "running":
      // a cancel asked for stays asked for, even if an older answer arrives
      return view.phase === "cancelling" ? { ...view, done } : { phase: "running", job: view.job, done, total, current };
    case "cancelling":
      return { phase: "cancelling", job: view.job, done, total };
    case "cancelled":
      return { phase: "cancelled", done, total };
    case "failed":
      return { phase: "failed", message: status.error ?? "The text couldn’t be read. Try again." };
    case "finished":
      // a cancel that came too late: the pages were all read, and they are
      // not added, because the person asked to stop
      return view.phase === "cancelling" ? { phase: "cancelled", done: total, total } : { phase: "applying", job: view.job };
  }
}
