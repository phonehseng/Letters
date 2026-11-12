// every call the AI Tools panel and Settings make into the shell (see
// app/shell/src/ai_commands.rs). errors arrive as { message, temporary }.

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { AiError, FoundPassage, Plan, ProviderStatus, TaskOutput } from "./aiPanel";

export type PageRange = { first: number; last: number } | null;

/** what the person asked for (letters_ai::TaskRequest). */
export type TaskRequest =
  | { task: "summarize"; pages: PageRange; length: "short" | "medium" | "detailed" }
  | { task: "keyPoints"; pages: PageRange; count: number }
  | { task: "ask"; question: string; pages: PageRange };

/** a rejection as the panel shows it. */
export function asAiError(problem: unknown): AiError {
  if (typeof problem === "object" && problem !== null && "message" in problem) {
    const error = problem as { message: string; temporary?: boolean };
    return { message: error.message, temporary: !!error.temporary };
  }
  return { message: String(problem), temporary: false };
}

export function aiStatus(): Promise<ProviderStatus> {
  return invoke("ai_status");
}

export function planRequest(id: number, request: TaskRequest): Promise<Plan> {
  return invoke("ai_plan", { id, request });
}

/** runs a plan; `confirmed` is the person's Send, `remember` the "Don’t ask again" box. */
export function runPlan(planId: number, confirmed: boolean, remember: boolean): Promise<TaskOutput> {
  return invoke("ai_run", { planId, confirmed, remember });
}

export function stopPlan(planId: number): Promise<void> {
  return invoke("ai_stop", { planId });
}

/** Find a Topic, on this computer, with no AI. */
export function findTopic(id: number, topic: string, pages: PageRange): Promise<FoundPassage[]> {
  return invoke("ai_find_offline", { id, topic, pages });
}

export type ProgressEvent = { planId: number } & ({ kind: "step"; done: number; total: number; stage: string } | { kind: "text"; text: string });

export function listenForAiProgress(onProgress: (event: ProgressEvent) => void): () => void {
  const stop = getCurrentWebviewWindow().listen<ProgressEvent>("ai-progress", (event) => onProgress(event.payload));
  return () => {
    stop.then((unlisten) => unlisten());
  };
}

export function testConnection(): Promise<string> {
  return invoke("ai_test_connection");
}

/** keeps a typed key in the system's password store; an empty one forgets it. */
export function saveKey(key: string): Promise<boolean> {
  return invoke("ai_save_key", { key });
}

export function hasKey(): Promise<boolean> {
  return invoke<boolean>("ai_has_key").catch(() => false);
}

export function startServer(): Promise<string> {
  return invoke("ai_start_server");
}

export function stopServer(): Promise<void> {
  return invoke("ai_stop_server");
}

export function serverRunning(): Promise<boolean> {
  return invoke<boolean>("ai_server_running").catch(() => false);
}
