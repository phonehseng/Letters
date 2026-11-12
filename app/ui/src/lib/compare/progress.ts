// how far a comparison has got, said plainly and as a share of the work.

import type { JobStatus, Stage } from "./compareApi";

const STAGE_WORDS: Record<Stage, string> = {
  readingOld: "Reading the old version",
  readingNew: "Reading the new version",
  comparingPages: "Comparing pages",
};

// reading a page is much quicker than comparing a pair, so the bar
// gives reading a smaller part of its length
const STAGE_SHARE: Record<Stage, [number, number]> = {
  readingOld: [0, 0.15],
  readingNew: [0.15, 0.3],
  comparingPages: [0.3, 1],
};

/** from 0 to 1, for the progress bar. */
export function progressFraction(status: JobStatus): number {
  if (status.state === "finished") return 1;
  if (status.state !== "running" || !status.stage) return 0;
  const [start, end] = STAGE_SHARE[status.stage];
  const within = status.total > 0 ? status.done / status.total : 0;
  return start + (end - start) * Math.min(1, within);
}

/** "Comparing pages: 120 of 500", or "Starting…". */
export function progressText(status: JobStatus): string {
  if (status.state !== "running") return "";
  if (!status.stage) return "Starting…";
  return `${STAGE_WORDS[status.stage]}: ${status.done} of ${status.total}`;
}

/** how often the window asks the shell how far it has got. */
export const POLL_MS = 120;
