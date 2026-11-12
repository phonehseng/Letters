<script lang="ts">
  // AI Tools in Settings: the master switch (off until turned on), where
  // the model runs, its address and name, the key, the context size, Test
  // Connection, and starting a model server program on this computer.
  // the key goes from its field straight to the system's password store;
  // it is never part of the settings file. names in the examples are
  // made up, as everywhere in Letters.
  import { onMount } from "svelte";
  import * as ai from "../ai/aiApi";
  import { chooseFile } from "./settingsApi";
  import { aiProblems, newProvider, withProviderKind, type AppSettings } from "./settingsModel";

  interface Props {
    settings: AppSettings;
    onChange: (update: (draft: AppSettings) => void) => Promise<void>;
  }

  let { settings, onChange }: Props = $props();

  let keyField = $state("");
  let hasKey = $state(false);
  let result = $state<{ text: string; ok: boolean } | null>(null);
  let busy = $state(false);
  let serverRunning = $state(false);

  const provider = $derived(settings.ai.provider);
  const problems = $derived(aiProblems(settings, hasKey));

  onMount(async () => {
    hasKey = await ai.hasKey();
    serverRunning = await ai.serverRunning();
  });

  function setKind(kind: "localServer" | "remoteApi") {
    onChange((draft) => {
      const switched = withProviderKind(draft, kind);
      draft.ai.provider = switched.ai.provider;
    });
  }

  function setField(field: string, value: string | number | boolean | null) {
    onChange((draft) => {
      const target = (draft.ai.provider ?? newProvider("localServer")) as unknown as Record<string, unknown>;
      target[field] = value;
      draft.ai.provider = target as unknown as AppSettings["ai"]["provider"];
    });
  }

  async function saveKey() {
    hasKey = await ai.saveKey(keyField);
    keyField = "";
    result = { text: hasKey ? "The key is kept in this computer’s password store." : "The key was removed.", ok: true };
  }

  async function test() {
    busy = true;
    result = null;
    try {
      result = { text: await ai.testConnection(), ok: true };
    } catch (problem) {
      result = { text: ai.asAiError(problem).message, ok: false };
    }
    busy = false;
  }

  async function chooseServer(field: "executable" | "modelFile") {
    const chosen = await chooseFile(field === "executable" ? "Model Server Program" : "Model File");
    if (chosen) await onChange((draft) => (draft.aiServer[field] = chosen));
  }

  async function startServer() {
    busy = true;
    result = { text: "Starting the model server. Loading a big model can take a minute…", ok: true };
    try {
      const address = await ai.startServer();
      serverRunning = true;
      result = { text: `The model server is running at ${address}. It stops when Letters quits.`, ok: true };
    } catch (problem) {
      result = { text: ai.asAiError(problem).message, ok: false };
    }
    busy = false;
  }

  async function stopServer() {
    await ai.stopServer();
    serverRunning = false;
    result = { text: "The model server stopped.", ok: true };
  }
</script>

<label class="check switch">
  <input type="checkbox" checked={settings.ai.enabled} onchange={(event) => onChange((draft) => (draft.ai.enabled = event.currentTarget.checked))} />
  Turn On AI Tools
</label>
<p class="hint">
  AI Tools summarize a document, list its key points and answer questions with quotes you can check. They need a language model: one on this computer, where nothing leaves it, or an online service with your own key, which gets the text it needs. Before
  anything is sent to a service, Letters shows what and where, and waits for you to choose Send. Find a Topic always works without them.
</p>

{#if settings.ai.enabled}
  <fieldset class="setting">
    <legend class="caption">Where the Model Runs</legend>
    <label class="check"><input type="radio" name="ai-kind" checked={provider?.kind === "localServer"} onchange={() => setKind("localServer")} /> Model on this computer</label>
    <label class="check"><input type="radio" name="ai-kind" checked={provider?.kind === "remoteApi"} onchange={() => setKind("remoteApi")} /> Online service, with your key</label>
  </fieldset>

  {#if provider}
    <label class="setting">
      <span class="caption">Server Address</span>
      <input
        class="field"
        value={provider.address}
        placeholder={provider.kind === "remoteApi" ? "https://api.example.com/v1" : "http://127.0.0.1:8080"}
        onchange={(event) => setField("address", event.currentTarget.value.trim())}
      />
    </label>
    <label class="setting">
      <span class="caption">Model Name{provider.kind === "localServer" ? " (optional)" : ""}</span>
      <input
        class="field"
        value={provider.model ?? ""}
        placeholder={provider.kind === "remoteApi" ? "my-remote-model" : "my-local-model"}
        onchange={(event) => setField("model", event.currentTarget.value.trim() || (provider.kind === "localServer" ? null : ""))}
      />
    </label>
    {#if provider.kind === "remoteApi"}
      <label class="setting">
        <span class="caption">Key {hasKey ? "(one is kept)" : ""}</span>
        <input class="field" type="password" autocomplete="off" bind:value={keyField} placeholder={hasKey ? "Paste a new key to replace it" : "Paste the key"} />
      </label>
      <div class="row">
        <button class="button" disabled={!keyField.trim()} onclick={saveKey}>Keep Key</button>
        {#if hasKey}<button class="button" onclick={() => ((keyField = ""), saveKey())}>Remove Key</button>{/if}
      </div>
    {/if}
    <label class="setting">
      <span class="caption">Context Size (tokens)</span>
      <input class="field" type="number" min="512" step="512" value={provider.contextTokens} onchange={(event) => setField("contextTokens", Number(event.currentTarget.value))} />
      <span class="hint">How much text the model reads at once. Use the number the model or the service gives.</span>
    </label>

    {#if problems.length > 0}
      <ul class="problems">
        {#each problems as problem (problem)}<li>{problem}</li>{/each}
      </ul>
    {/if}
    <div class="row">
      <button class="button" disabled={busy || problems.length > 0} onclick={test}>Test Connection</button>
    </div>
    <p class="hint">Test Connection sends one fixed sentence{provider.address ? ` to ${provider.address}` : ""}, never any document text.</p>
  {/if}

  {#if provider?.kind === "localServer"}
    <fieldset class="setting">
      <legend class="caption">Start a Model Server on This Computer</legend>
      <span class="hint">Letters can start a server program you already have, with a model file you already have, on this computer only. Letters never downloads either.</span>
      <div class="row">
        <button class="button" onclick={() => chooseServer("executable")}>Choose Program…</button>
        <span class="path">{settings.aiServer.executable || "No program chosen"}</span>
      </div>
      <div class="row">
        <button class="button" onclick={() => chooseServer("modelFile")}>Choose Model File…</button>
        <span class="path">{settings.aiServer.modelFile || "No model file chosen"}</span>
      </div>
      <div class="row">
        {#if serverRunning}
          <button class="button" onclick={stopServer}>Stop Server</button>
        {:else}
          <button class="button" disabled={busy || !settings.aiServer.executable || !settings.aiServer.modelFile} onclick={startServer}>Start Server</button>
        {/if}
      </div>
    </fieldset>
  {/if}

  {#if result}
    <p class:problem={!result.ok} role="status">{result.text}</p>
  {/if}
{/if}

<style>
  .switch {
    font: var(--headline);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.375rem;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .setting .field {
    width: min(22rem, 100%);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .path {
    color: var(--label-secondary);
    font: var(--subheadline);
    overflow-wrap: anywhere;
  }

  .problems {
    margin: 0;
    padding-left: 1.25rem;
    color: var(--danger);
  }

  p {
    margin: 0;
  }

  .problem {
    color: var(--danger);
  }
</style>
