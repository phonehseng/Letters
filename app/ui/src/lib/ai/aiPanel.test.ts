// tests of the AI Tools panel's logic: the steps of a request, the header line, citation chips.

import { describe, expect, it } from "vitest";
import { answerParts, headerLine, IDLE, next, pageRangeFrom, pagesRead, tabsFor, type Plan, type RequestState } from "./aiPanel";

function plan(askFirst: boolean): Plan {
  return {
    planId: 7,
    askFirst,
    disclosure: {
      leavesDevice: askFirst,
      destination: askFirst ? "api.example.com" : "127.0.0.1",
      providerName: askFirst ? "Online service" : "Model on this computer",
      pages: [1, 2],
      characters: 400,
      words: 80,
      preview: "Photosynthesis turns light…",
      sentence: askFirst ? "This will send the text of pages 1–2 (about 80 words) to api.example.com." : "This stays on this computer.",
    },
  };
}

function run(events: Parameters<typeof next>[1][], from: RequestState = IDLE): RequestState {
  return events.reduce(next, from);
}

describe("a request's steps", () => {
  it("text that leaves waits for Send", () => {
    const waiting = run([{ type: "ask" }, { type: "planned", plan: plan(true) }]);
    expect(waiting.step).toBe("confirming");
    // nothing else moves it on: no text arrives, no answer is shown
    expect(run([{ type: "text", text: "x" }, { type: "finished", output: { kind: "keyPoints", result: [] } }], waiting).step).toBe("confirming");
    expect(next(waiting, { type: "send" }).step).toBe("running");
  });

  it("cancel in the confirmation sends nothing and goes back", () => {
    const waiting = run([{ type: "ask" }, { type: "planned", plan: plan(true) }]);
    expect(next(waiting, { type: "cancel" })).toEqual(IDLE);
  });

  it("a model on this computer, or a remembered yes, runs at once", () => {
    expect(run([{ type: "ask" }, { type: "planned", plan: plan(false) }]).step).toBe("running");
  });

  it("the answer streams in, then the finished output replaces it", () => {
    const state = run([{ type: "ask" }, { type: "planned", plan: plan(false) }, { type: "text", text: "Light " }, { type: "text", text: "is used." }]);
    expect(state.step === "running" && state.streamed).toBe("Light is used.");
    const done = next(state, { type: "finished", output: { kind: "summary", result: { text: "Light is used.", pages: { first: 1, last: 2 }, parts: 1 } } });
    expect(done.step).toBe("done");
  });

  it("stop and errors end a request; a second ask while running is ignored", () => {
    const busy = run([{ type: "ask" }, { type: "planned", plan: plan(false) }]);
    expect(next(busy, { type: "ask" })).toBe(busy);
    expect(next(busy, { type: "stop" }).step).toBe("stopped");
    const failed = next(busy, { type: "failed", error: { message: "The server took too long to answer.", temporary: true } });
    expect(failed).toEqual({ step: "failed", error: { message: "The server took too long to answer.", temporary: true } });
  });
});

describe("the header and the answer", () => {
  it("names the provider and whether text leaves", () => {
    expect(headerLine({ enabled: true, ready: true, providerName: "Online service", destination: "api.example.com", leavesDevice: true, problem: null })).toEqual({
      title: "Online service",
      detail: "Text you send goes to api.example.com.",
      leaves: true,
    });
    expect(headerLine({ enabled: true, ready: true, providerName: "Model on this computer", destination: "127.0.0.1", leavesDevice: false, problem: null }).detail).toBe("This stays on this computer.");
    expect(headerLine(null).title).toBe("Not set up");
  });

  it("cuts an answer into words and citation chips", () => {
    const citation = { marker: 1, page: 4, rect: { x: 1, y: 2, width: 3, height: 4 }, quote: "light" };
    const parts = answerParts({ text: "It uses light [1]. Also [9].", citations: [citation], unverifiedQuotes: [], found: true });
    expect(parts).toEqual([
      { kind: "text", text: "It uses light " },
      { kind: "citation", citation },
      { kind: "text", text: ". Also [9]." },
    ]);
  });

  it("page ranges as people type them", () => {
    expect(pageRangeFrom("", 10)).toEqual({ ok: true, range: null });
    expect(pageRangeFrom("3-5", 10)).toEqual({ ok: true, range: { first: 3, last: 5 } });
    expect(pageRangeFrom("4", 10)).toEqual({ ok: true, range: { first: 4, last: 4 } });
    expect(pageRangeFrom("12", 10).ok).toBe(false);
    expect(pageRangeFrom("a", 10).ok).toBe(false);
    expect(pagesRead({ text: "", pages: { first: 2, last: 2 }, parts: 1 })).toBe("From page 2.");
  });

  it("only Find a Topic shows while the tools are off", () => {
    expect(tabsFor(false)).toEqual(["findTopic"]);
    expect(tabsFor(true)).toEqual(["summarize", "keyPoints", "ask", "findTopic"]);
  });
});
