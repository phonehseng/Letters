// the fix wizard: one fixable finding at a time, with Skip and Back, and
// "3 of 11" as it goes. after every fix the document is checked again; a
// step whose problem is gone counts as fixed, and problems that only show
// up now (pictures that need alt text once tags exist) join at the end.

import type { Finding } from "./a11yApi";
import { findingKey, wizardFindings } from "./findings";
import type { Report } from "./a11yApi";

export type StepStatus = "open" | "fixed" | "skipped";

export interface WizardStep {
  key: string;
  finding: Finding;
  status: StepStatus;
}

export interface WizardState {
  steps: WizardStep[];
  /** the step on screen; steps.length means the wizard is at its end */
  index: number;
}

export function startWizard(report: Report): WizardState {
  const steps = wizardFindings(report).map((finding) => ({ key: findingKey(finding), finding, status: "open" as const }));
  return { steps, index: 0 };
}

export function currentStep(state: WizardState): WizardStep | null {
  return state.steps[state.index] ?? null;
}

export function isFinished(state: WizardState): boolean {
  return state.index >= state.steps.length;
}

/** "3 of 11", counting the step on screen. */
export function progressText(state: WizardState): string {
  if (state.steps.length === 0) return "Nothing to fix";
  if (isFinished(state)) return `${state.steps.length} of ${state.steps.length}`;
  return `${state.index + 1} of ${state.steps.length}`;
}

/** how many steps are fixed, for the closing sentence. */
export function fixedCount(state: WizardState): number {
  return state.steps.filter((step) => step.status === "fixed").length;
}

/** Skip: leave this one as it is and go on. */
export function skip(state: WizardState): WizardState {
  const steps = state.steps.map((step, index) => (index === state.index && step.status === "open" ? { ...step, status: "skipped" as const } : step));
  return { steps, index: Math.min(state.index + 1, steps.length) };
}

/** Back: the step before, whatever happened to it. */
export function back(state: WizardState): WizardState {
  return { ...state, index: Math.max(0, state.index - 1) };
}

/**
 * takes a fresh check's report. steps whose problem is gone are fixed; a
 * fixed step whose problem came back (after Undo) is open again; new
 * fixable problems are added at the end. the step on screen stays put.
 */
export function refresh(state: WizardState, report: Report): WizardState {
  const fresh = wizardFindings(report);
  const present = new Map(fresh.map((finding) => [findingKey(finding), finding]));
  const steps: WizardStep[] = state.steps.map((step) => {
    const finding = present.get(step.key);
    if (!finding) return { ...step, status: "fixed" };
    return { ...step, finding, status: step.status === "fixed" ? "open" : step.status };
  });
  const known = new Set(steps.map((step) => step.key));
  for (const finding of fresh) {
    const key = findingKey(finding);
    if (!known.has(key)) steps.push({ key, finding, status: "open" });
  }
  return { steps, index: Math.min(state.index, steps.length) };
}

/** after a fix: the next step still open after this one, or the end. */
export function advance(state: WizardState): WizardState {
  for (let index = state.index; index < state.steps.length; index += 1) {
    if (state.steps[index].status === "open") return { ...state, index };
  }
  return { ...state, index: state.steps.length };
}

/** the sentence at the end of the wizard. */
export function closingText(state: WizardState): string {
  const fixed = fixedCount(state);
  const skipped = state.steps.filter((step) => step.status === "skipped").length;
  const fixedPart = `${fixed} of ${state.steps.length} fixed`;
  if (skipped === 0) return `${fixedPart}.`;
  return `${fixedPart}, ${skipped} skipped. Skipped problems are still in the list.`;
}
