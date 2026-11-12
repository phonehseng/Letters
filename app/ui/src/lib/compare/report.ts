// what Save Report… needs besides the comparison: where the report is
// suggested to go, and what it says about the two versions.

import { pathBeside, stemOf } from "../pageApi";
import type { ReportFormat, ReportOptions } from "./compareApi";

const EXTENSIONS: Record<ReportFormat, string> = { pdf: "pdf", html: "html", text: "txt" };

/** beside the newer file when it is a file: "essay v2 - changes.pdf". */
export function suggestedReportPath(newerPath: string | null, format: ReportFormat): string {
  const extension = EXTENSIONS[format];
  if (!newerPath) return `Changes.${extension}`;
  return pathBeside(newerPath, `${stemOf(newerPath)} - changes.${extension}`);
}

/** the report's words about the versions; the engine never reads the clock itself. */
export function reportOptions(oldName: string, newName: string, thumbnails: boolean, today: Date, language: string): ReportOptions {
  const madeOn = today.toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" });
  return {
    title: "Comparison report",
    oldName,
    newName,
    madeOn,
    thumbnails,
    language: language || "en",
  };
}

/** what Explain lists on the Compare screen. */
export const EXPLAINED_ON_COMPARE = [
  "compareBack",
  "compareSideBySide",
  "compareOverlay",
  "compareBlend",
  "compareFlash",
  "compareLinkScrolling",
  "compareChanges",
  "compareNextChange",
  "comparePreviousChange",
  "compareOptions",
  "compareSaveReport",
  "zoomIn",
  "zoomOut",
  "fitWidth",
] as const;
