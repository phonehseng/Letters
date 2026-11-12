// Strict Word mode: only features Microsoft Word keeps exactly.
//
// the list should come from the office engine's feature_matrix(); until
// that exists this adapter holds Letters' own list, with the reason shown
// in the tooltip of anything strict mode turns off. one function to swap.

import type { ToolName } from "../tools";
import type { DocumentMeta, DocumentModel } from "./model";

export interface FeatureRule {
  tool: ToolName;
  /** true when Word shows this exactly as Letters does */
  wordExact: boolean;
  /** why not, in plain words, for the tooltip */
  reason?: string;
}

/** the adapter: the engine's feature matrix when it lands, this list until then. */
export function featureMatrix(): FeatureRule[] {
  return [
    { tool: "wColumns", wordExact: false, reason: "The editing surface shows one column, so what you see would differ from Word." },
    { tool: "nightMode", wordExact: true },
    { tool: "wImageWrap", wordExact: true },
    { tool: "wPresetMla", wordExact: true },
    { tool: "wPresetApa", wordExact: true },
  ];
}

/** whether a tool is offered in a mode, and the tooltip when it isn't. */
export function gate(tool: ToolName, mode: "free" | "strict", matrix: FeatureRule[] = featureMatrix()): { enabled: boolean; reason?: string } {
  if (mode === "free") return { enabled: true };
  const rule = matrix.find((each) => each.tool === tool);
  if (!rule || rule.wordExact) return { enabled: true };
  return { enabled: false, reason: `Off in Strict Word mode: ${rule.reason ?? "Word would show it differently."}` };
}

export interface CompatibilityIssue {
  what: string;
}

/**
 * what would look different in Word (the adapter for the engine's
 * word_compatibility; until it lands: content kept from an OpenDocument
 * file, which Word can't show, and pictures that are missing).
 */
export function compatibilityIssues(model: DocumentModel | DocumentMeta & { blocks?: unknown[] }, fromOdt: boolean): CompatibilityIssue[] {
  const issues: CompatibilityIssue[] = [];
  if (fromOdt) {
    const text = JSON.stringify(model.blocks ?? []);
    const kept = (text.match(/"type":"preserved"/g) ?? []).length + (text.match(/"type":"preservedRun"/g) ?? []).length;
    if (kept) issues.push({ what: `${kept} item${kept === 1 ? "" : "s"} from the OpenDocument file that Word can’t show` });
  }
  const sections = model.sections ?? [];
  if (sections.some((section) => (section.columns?.count ?? 1) > 1)) issues.push({ what: "columns, which the editing surface shows as one" });
  return issues;
}

/** "Everything here opens the same in Word." or "2 things will look different in Word". */
export function compatibilityLine(issues: CompatibilityIssue[]): string {
  if (issues.length === 0) return "Everything here opens the same in Word.";
  return `${issues.length} thing${issues.length === 1 ? "" : "s"} will look different in Word`;
}
