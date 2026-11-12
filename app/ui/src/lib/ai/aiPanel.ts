// the AI Tools panel's logic, without any dom: the steps of a request
// (plan, confirm when text would leave, run, answer), the words in the
// panel's header, and an answer cut into text and citation chips.
//
// the shell enforces the rule too (shell-core/src/ai/desk.rs): a plan
// that sends text away runs only with the person's Send or an earlier
// "Don’t ask again" for that provider and document.

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** what the engine says it would send (letters_ai::Disclosure). */
export interface Disclosure {
  leavesDevice: boolean;
  destination: string;
  providerName: string;
  pages: number[];
  characters: number;
  words: number;
  preview: string;
  sentence: string;
}

export interface Plan {
  planId: number;
  disclosure: Disclosure;
  /** show the confirmation before running */
  askFirst: boolean;
}

export interface Citation {
  marker: number;
  page: number;
  rect: Rect;
  quote: string;
}

export interface Answer {
  text: string;
  citations: Citation[];
  unverifiedQuotes: string[];
  found: boolean;
}

export interface Summary {
  text: string;
  pages: { first: number; last: number };
  parts: number;
}

export interface KeyPoint {
  text: string;
  pages: number[];
}

export interface FoundPassage {
  page: number;
  rect: Rect;
  text: string;
  score: number;
}

export type TaskOutput =
  | { kind: "summary"; result: Summary }
  | { kind: "keyPoints"; result: KeyPoint[] }
  | { kind: "answer"; result: Answer }
  | { kind: "passages"; result: FoundPassage[] };

export interface AiError {
  message: string;
  /** try again may help; otherwise Settings is the way out */
  temporary: boolean;
}

/** where a request is. */
export type RequestState =
  | { step: "idle" }
  | { step: "planning" }
  | { step: "confirming"; plan: Plan }
  | { step: "running"; plan: Plan; streamed: string; status: string }
  | { step: "done"; output: TaskOutput }
  | { step: "failed"; error: AiError }
  | { step: "stopped" };

export type RequestEvent =
  | { type: "ask" }
  | { type: "planned"; plan: Plan }
  | { type: "send" }
  | { type: "cancel" }
  | { type: "text"; text: string }
  | { type: "status"; status: string }
  | { type: "finished"; output: TaskOutput }
  | { type: "failed"; error: AiError }
  | { type: "stop" };

export const IDLE: RequestState = { step: "idle" };

/** the next state. anything that doesn't fit the current step is ignored. */
export function next(state: RequestState, event: RequestEvent): RequestState {
  switch (event.type) {
    case "ask":
      return state.step === "planning" || state.step === "running" ? state : { step: "planning" };
    case "planned":
      if (state.step !== "planning") return state;
      return event.plan.askFirst ? { step: "confirming", plan: event.plan } : running(event.plan);
    case "send":
      return state.step === "confirming" ? running(state.plan) : state;
    case "cancel":
      return state.step === "confirming" ? IDLE : state;
    case "text":
      return state.step === "running" ? { ...state, streamed: state.streamed + event.text } : state;
    case "status":
      return state.step === "running" ? { ...state, status: event.status } : state;
    case "finished":
      return state.step === "running" ? { step: "done", output: event.output } : state;
    case "failed":
      return state.step === "planning" || state.step === "running" ? { step: "failed", error: event.error } : state;
    case "stop":
      return state.step === "running" || state.step === "planning" ? { step: "stopped" } : state;
  }
}

function running(plan: Plan): RequestState {
  return { step: "running", plan, streamed: "", status: "Starting…" };
}

/** what the panel's header says about where text goes. */
export interface ProviderStatus {
  enabled: boolean;
  ready: boolean;
  providerName: string | null;
  destination: string | null;
  leavesDevice: boolean;
  problem: string | null;
}

export function headerLine(status: ProviderStatus | null): { title: string; detail: string; leaves: boolean } {
  if (!status || !status.ready) {
    return { title: "Not set up", detail: status?.problem ?? "Choose where AI Tools run in Settings.", leaves: false };
  }
  if (status.leavesDevice) {
    return { title: status.providerName ?? "Online service", detail: `Text you send goes to ${status.destination}.`, leaves: true };
  }
  return { title: status.providerName ?? "Model on this computer", detail: "This stays on this computer.", leaves: false };
}

/** a piece of an answer: words, or a chip for citation n. */
export type AnswerPart = { kind: "text"; text: string } | { kind: "citation"; citation: Citation };

/** cuts "Light is captured [1] and stored [2]." into text and chips. markers with no citation stay text. */
export function answerParts(answer: Answer): AnswerPart[] {
  const parts: AnswerPart[] = [];
  const pattern = /\[(\d+)\]/g;
  let at = 0;
  for (const match of answer.text.matchAll(pattern)) {
    const citation = answer.citations.find((each) => each.marker === Number(match[1]));
    if (!citation) continue;
    const index = match.index ?? 0;
    if (index > at) parts.push({ kind: "text", text: answer.text.slice(at, index) });
    parts.push({ kind: "citation", citation });
    at = index + match[0].length;
  }
  if (at < answer.text.length) parts.push({ kind: "text", text: answer.text.slice(at) });
  return parts;
}

/** the chip's words: "Page 4". */
export function citationLabel(citation: Citation): string {
  return `Page ${citation.page}`;
}

/** the engine's answer when nothing in the document matches. */
export const DOESNT_SAY = "The document doesn’t say.";

/** "pages 3–5" from a summary's range, for the line under it. */
export function pagesRead(summary: Summary): string {
  const { first, last } = summary.pages;
  return first === last ? `From page ${first}.` : `From pages ${first}–${last}.`;
}

/** a page range typed in the panel ("3-5"), as the engine's 1-based range, or null for every page. */
export function pageRangeFrom(text: string, pageCount: number): { ok: true; range: { first: number; last: number } | null } | { ok: false; error: string } {
  const trimmed = text.trim();
  if (trimmed === "") return { ok: true, range: null };
  const match = /^(\d+)\s*(?:[-–]\s*(\d+))?$/.exec(trimmed);
  if (!match) return { ok: false, error: "Type one page or a run of pages, such as 3-5." };
  const first = Number(match[1]);
  const last = match[2] ? Number(match[2]) : first;
  if (first < 1 || last < first || first > pageCount) return { ok: false, error: `Choose pages from 1 to ${pageCount}.` };
  return { ok: true, range: { first, last: Math.min(last, pageCount) } };
}

/** the tabs, in order; only Find a Topic shows while the tools are off. */
export type AiTab = "summarize" | "keyPoints" | "ask" | "findTopic";

export function tabsFor(enabled: boolean): AiTab[] {
  return enabled ? ["summarize", "keyPoints", "ask", "findTopic"] : ["findTopic"];
}
