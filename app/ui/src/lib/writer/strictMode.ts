// Strict Word mode: only features Microsoft Word keeps exactly.
//
// the single source of truth is the office engine: compat::feature_matrix()
// says for every editor feature whether word keeps it exactly, and
// compat::word_compatibility() lists what in this document word would
// show differently, with a fix where there is one (writerApi.compatibility).
// this file maps the writer's tools to the engine's feature keys and turns
// the answers into what the screen shows.

import type { ToolName } from "../tools";
import type { CompatibilityIssue, EngineFeature } from "./writerApi";

/** the engine feature each gated tool stands for. */
export const FEATURE_FOR_TOOL: Partial<Record<ToolName, string>> = {
  wColumns: "columns",
  wHighlight: "highlight",
  wImageWrap: "floatingPictures",
  wImageFree: "floatingPictures",
  wImageCrop: "pictureCrop",
  wImageRotate: "pictureRotation",
  wTextBlockSplit: "textBoxes",
  wTextBlockMake: "textBoxes",
  wTrackChanges: "trackChanges",
  wInsertPageBreak: "inlinePageBreak",
  wLists: "lists",
  wTableMerge: "tables",
  wInsertFootnote: "footnotes",
  wHeaderFooter: "headersFooters",
} as Partial<Record<ToolName, string>>;

/** offered in strict mode: exact in word files (the engine's Feature::word_exact). */
export function wordExact(feature: EngineFeature): boolean {
  return feature.support === "both" || feature.support === "wordExact";
}

/** whether a tool is offered in a mode, and the tooltip when it isn't. */
export function gate(tool: ToolName, mode: "free" | "strict", matrix: EngineFeature[]): { enabled: boolean; reason?: string } {
  if (mode === "free") return { enabled: true };
  const key = FEATURE_FOR_TOOL[tool];
  const feature = key ? matrix.find((each) => each.key === key) : undefined;
  if (!feature || wordExact(feature)) return { enabled: true };
  const why = feature.note ? `${capitalise(feature.note)}.` : "Word would show it differently.";
  return { enabled: false, reason: `Off in Strict Word mode: ${why}` };
}

function capitalise(text: string): string {
  return text ? text[0].toUpperCase() + text.slice(1) : text;
}

/** "Everything here opens the same in Word." or "2 things will look different in Word". */
export function compatibilityLine(issues: CompatibilityIssue[]): string {
  if (issues.length === 0) return "Everything here opens the same in Word.";
  return `${issues.length} thing${issues.length === 1 ? "" : "s"} will look different in Word`;
}

/** how many of the issues Letters can fix by itself. */
export function fixableCount(issues: CompatibilityIssue[]): number {
  return issues.filter((issue) => !!issue.fix).length;
}

/** the question asked when switching to Strict Word mode with issues left. */
export function switchQuestion(issues: CompatibilityIssue[]): string | null {
  if (issues.length === 0) return null;
  const fixable = fixableCount(issues);
  const total = `${issues.length} thing${issues.length === 1 ? "" : "s"} in this document ${issues.length === 1 ? "isn’t" : "aren’t"} Word-exact.`;
  if (fixable === 0) return `${total} Letters can’t change them for you; Show lists them.`;
  if (fixable === issues.length) return `${total} Fix All changes ${issues.length === 1 ? "it" : "them"} so Word shows exactly what you see. Undo takes it back.`;
  return `${total} Fix All changes the ${fixable} Letters can fix; Show lists the rest.`;
}

/** the label of an issue's fix button, from the engine's fix kind. */
export function fixLabel(issue: CompatibilityIssue): string | null {
  switch (issue.fix?.type) {
    case "keepTextOnly":
      return "Keep Text";
    case "nearestHighlight":
      return "Use Word’s Colour";
    case "decimalNumbers":
      return "Use Numbers";
    case "replaceFont":
      return `Use ${String(issue.fix.to ?? "a Standard Font")}`;
    case "dissolveInnerTextBox":
      return "Take Text Out";
    default:
      return null;
  }
}
