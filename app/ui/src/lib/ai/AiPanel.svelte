<script lang="ts">
  // the AI Tools panel, on the trailing side like Explain. with the tools
  // off (the default) it shows only Find a Topic, which works on this
  // computer with no AI. with them on: Summarize, Key Points, Ask and Find
  // a Topic, under a header that names the provider and says whether text
  // leaves the computer. before anything is sent away, the engine's own
  // sentence and a preview ask for Send or Cancel (DisclosureSheet).
  // the steps live in aiPanel.ts.
  import { onMount, untrack } from "svelte";
  import Icon from "../Icon.svelte";
  import Choices from "../inspector/Choices.svelte";
  import * as api from "./aiApi";
  import {
    answerParts,
    citationLabel,
    headerLine,
    IDLE,
    next,
    pageRangeFrom,
    pagesRead,
    tabsFor,
    type AiTab,
    type FoundPassage,
    type ProviderStatus,
    type Rect,
    type RequestEvent,
    type RequestState,
  } from "./aiPanel";
  import DisclosureSheet from "./DisclosureSheet.svelte";

  interface Props {
    documentId: number;
    pageCount: number;
    /** the tools are on in Settings */
    enabled: boolean;
    /** the tab a command asked for */
    tab?: AiTab;
    /** words to find a topic for at once, from the find bar */
    initialTopic?: string;
    /** shows a passage: scrolls to it and outlines it. page counted from 1 */
    onShowPassage: (page: number, rect: Rect) => void;
    onOpenSettings: () => void;
    onClose: () => void;
  }

  let { documentId, pageCount, enabled, tab = $bindable(), initialTopic = "", onShowPassage, onOpenSettings, onClose }: Props = $props();

  const LABELS: Record<AiTab, string> = { summarize: "Summarize", keyPoints: "Key Points", ask: "Ask", findTopic: "Find a Topic" };

  let status = $state<ProviderStatus | null>(null);
  let request = $state<RequestState>(IDLE);
  let lastAsk: (() => void) | null = null;
  let length = $state<"short" | "medium" | "detailed">("short");
  let pagesText = $state("");
  let question = $state("");
  let topic = $state(untrack(() => initialTopic));
  let passages = $state<FoundPassage[] | null>(null);
  let findProblem = $state<string | null>(null);

  const tabs = $derived(tabsFor(enabled));
  const header = $derived(headerLine(status));
  const range = $derived(pageRangeFrom(pagesText, pageCount));
  const busy = $derived(request.step === "planning" || request.step === "running");

  function send(event: RequestEvent) {
    request = next(request, event);
  }

  onMount(() => {
    if (!tab) tab = enabled ? "summarize" : "findTopic";
    if (enabled) api.aiStatus().then((found) => (status = found));
    if (tab === "findTopic" && topic.trim()) find();
    return api.listenForAiProgress((event) => {
      if (request.step !== "running" || event.planId !== request.plan.planId) return;
      if (event.kind === "text") send({ type: "text", text: event.text });
      else send({ type: "status", status: `${event.stage} (${event.done} of ${event.total})` });
    });
  });

  /** plan, maybe confirm, run: the same for every tab. */
  async function ask(make: () => api.TaskRequest) {
    if (!range.ok) return;
    lastAsk = () => ask(make);
    send({ type: "ask" });
    try {
      const plan = await api.planRequest(documentId, make());
      send({ type: "planned", plan });
      if (!plan.askFirst) await run(false, false);
    } catch (problem) {
      send({ type: "failed", error: api.asAiError(problem) });
    }
  }

  async function run(confirmed: boolean, remember: boolean) {
    if (request.step !== "running") return;
    const planId = request.plan.planId;
    try {
      const output = await api.runPlan(planId, confirmed, remember);
      send({ type: "finished", output });
    } catch (problem) {
      send({ type: "failed", error: api.asAiError(problem) });
    }
  }

  function confirm(remember: boolean) {
    send({ type: "send" });
    run(true, remember);
  }

  function stop() {
    if (request.step === "running") api.stopPlan(request.plan.planId);
    send({ type: "stop" });
  }

  const pages = () => (range.ok ? range.range : null);

  async function find() {
    findProblem = null;
    passages = null;
    try {
      passages = await api.findTopic(documentId, topic, range.ok ? range.range : null);
    } catch (problem) {
      findProblem = api.asAiError(problem).message;
    }
  }
</script>

<aside class="panel" aria-label="AI Tools">
  <header>
    <h2>{enabled ? "AI Tools" : "Find a Topic"}</h2>
    <button class="close" title="Close this panel" aria-label="Close" onclick={onClose}>
      <Icon name="close" size={0.75} />
    </button>
  </header>

  {#if enabled}
    <!-- always visible: who reads the text, and whether it leaves -->
    <div class="provider" class:leaves={header.leaves} role="status">
      <strong>{header.title}</strong>
      <span>{header.detail}</span>
    </div>
    <div class="tabs" role="tablist" aria-label="AI Tools">
      {#each tabs as each (each)}
        <button class="tab" role="tab" aria-selected={tab === each} class:selected={tab === each} onclick={() => ((tab = each), (request = IDLE))}>{LABELS[each]}</button>
      {/each}
    </div>
  {:else}
    <p class="hint">Works on this computer. No AI, and nothing is sent anywhere.</p>
  {/if}

  <label class="setting">
    <span class="caption">Pages</span>
    <input class="field" placeholder="All pages, or such as 3-5" bind:value={pagesText} />
  </label>
  {#if !range.ok}<p class="problem">{range.error}</p>{/if}

  {#if tab === "summarize"}
    <Choices label="Length" options={[{ value: "short", label: "Short" }, { value: "medium", label: "Medium" }, { value: "detailed", label: "Detailed" }]} bind:value={length} />
    <button class="button primary" disabled={busy || !range.ok || !status?.ready} onclick={() => ask(() => ({ task: "summarize", pages: pages(), length }))}>Summarize</button>
  {:else if tab === "keyPoints"}
    <button class="button primary" disabled={busy || !range.ok || !status?.ready} onclick={() => ask(() => ({ task: "keyPoints", pages: pages(), count: 6 }))}>List Key Points</button>
  {:else if tab === "ask"}
    <form onsubmit={(event) => (event.preventDefault(), question.trim() && ask(() => ({ task: "ask", question, pages: pages() })))}>
      <label class="setting">
        <span class="caption">Question</span>
        <textarea class="field" rows="3" placeholder="What does it say about…?" bind:value={question}></textarea>
      </label>
      <button class="button primary" type="submit" disabled={busy || !question.trim() || !status?.ready}>Ask</button>
    </form>
  {:else}
    <form onsubmit={(event) => (event.preventDefault(), topic.trim() && find())}>
      <label class="setting">
        <span class="caption">Topic</span>
        <input class="field" placeholder="such as the water cycle" bind:value={topic} />
      </label>
      <button class="button primary" type="submit" disabled={!topic.trim() || !range.ok}>Find</button>
    </form>
    {#if findProblem}<p class="problem">{findProblem}</p>{/if}
    {#if passages}
      {#if passages.length === 0}
        <p class="hint">No passage is about that. Try other words.</p>
      {/if}
      <ul class="passages">
        {#each passages as passage, index (index)}
          <li>
            <button class="passage" onclick={() => onShowPassage(passage.page, passage.rect)}>
              <span class="chip">Page {passage.page}</span>
              <span>{passage.text}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if tab !== "findTopic"}
    <div class="output" aria-live="polite">
      {#if request.step === "planning"}
        <p class="hint">Getting ready…</p>
      {:else if request.step === "running"}
        <p class="hint">{request.status}</p>
        {#if request.streamed}<p class="streamed">{request.streamed}</p>{/if}
        <button class="button" onclick={stop}>Stop</button>
      {:else if request.step === "stopped"}
        <p class="hint">Stopped.</p>
      {:else if request.step === "failed"}
        <p class="problem" role="alert">{request.error.message}</p>
        {#if request.error.temporary}
          <button class="button" onclick={() => lastAsk?.()}>Try Again</button>
        {:else}
          <button class="button" onclick={onOpenSettings}>Open Settings</button>
        {/if}
      {:else if request.step === "done"}
        {@const output = request.output}
        {#if output.kind === "summary"}
          <p class="answer">{output.result.text}</p>
          <p class="hint">{pagesRead(output.result)}</p>
        {:else if output.kind === "keyPoints"}
          <ul class="points">
            {#each output.result as point, index (index)}
              <li>
                {point.text}
                {#each point.pages as page (page)}
                  <span class="chip quiet">p. {page}</span>
                {/each}
              </li>
            {/each}
          </ul>
        {:else if output.kind === "answer"}
          {#if !output.result.found}
            <p class="answer">The document doesn’t say.</p>
          {:else}
            <p class="answer">
              {#each answerParts(output.result) as part, index (index)}
                {#if part.kind === "text"}{part.text}{:else}<button
                    class="chip"
                    title={`“${part.citation.quote}”`}
                    onclick={() => onShowPassage(part.citation.page, part.citation.rect)}>{citationLabel(part.citation)}</button
                  >{/if}
              {/each}
            </p>
            {#if output.result.unverifiedQuotes.length > 0}
              <p class="unverified">
                {output.result.unverifiedQuotes.length === 1 ? "One quote" : `${output.result.unverifiedQuotes.length} quotes`} couldn’t be found in the document, so {output.result.unverifiedQuotes.length === 1 ? "it isn’t" : "they aren’t"} linked: {output.result.unverifiedQuotes.map((quote) => `“${quote}”`).join(", ")}
              </p>
            {/if}
          {/if}
        {/if}
      {/if}
    </div>
  {/if}
</aside>

{#if request.step === "confirming"}
  <DisclosureSheet disclosure={request.plan.disclosure} onSend={confirm} onCancel={() => send({ type: "cancel" })} />
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    width: min(19rem, 45vw);
    height: 100%;
    overflow-y: auto;
    padding: 1rem 1rem 1.5rem;
    border-left: 0.5px solid var(--separator);
    background: var(--glass);
    backdrop-filter: var(--blur);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .close {
    display: grid;
    place-items: center;
    width: 1.5rem;
    height: 1.5rem;
    border: 0;
    border-radius: 50%;
    background: var(--control);
    color: var(--label-secondary);
  }

  .provider {
    display: flex;
    flex-direction: column;
    padding: 0.5rem 0.625rem;
    border-radius: var(--radius-group);
    background: var(--control);
    font: var(--subheadline);
    letter-spacing: var(--subheadline-spacing);
  }

  /* text leaving the computer is said plainly, never hidden in grey */
  .provider.leaves span {
    font-weight: 600;
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .tab {
    flex: 1;
    height: 1.5rem;
    padding: 0 0.375rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
  }

  .tab.selected {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  textarea {
    resize: vertical;
  }

  .problem,
  .unverified {
    margin: 0;
    color: var(--danger);
  }

  .output p {
    margin: 0 0 0.5rem;
  }

  .answer,
  .streamed {
    line-height: 1.25rem;
  }

  .chip {
    display: inline-block;
    margin: 0 0.125rem;
    padding: 0 0.375rem;
    border: 0;
    border-radius: 0.625rem;
    background: var(--accent-soft);
    color: var(--accent-label);
    font: var(--footnote);
    font-weight: 600;
    white-space: nowrap;
  }

  .chip.quiet {
    background: var(--control);
    color: var(--label-secondary);
  }

  .passages,
  .points {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .points li {
    margin-bottom: 0.5rem;
  }

  .passage {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.25rem;
    width: 100%;
    padding: 0.5rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .passage:hover {
    background: var(--control-hover);
  }
</style>
