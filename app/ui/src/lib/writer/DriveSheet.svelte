<script lang="ts">
  // Save to Google Drive: two ways, each said plainly.
  // 1. the Google Drive folder (Google Drive for desktop): a normal save
  //    into that folder, which Google's app then uploads. no sign-in here,
  //    but it can't make a native Google Doc.
  // 2. direct upload through Google's api: needs a client id in Settings,
  //    signs in in the browser, and uploads one file each time the person
  //    confirms exactly what goes where.
  import { onMount } from "svelte";
  import { tooltipFor } from "../tools";
  import { formatForPath, saveFilters } from "./formats";
  import * as api from "./writerApi";
  import { errorText, type WriterSession } from "./writerSession.svelte";

  interface Props {
    session: WriterSession;
    top: number;
    onClose: () => void;
  }

  let { session, top, onClose }: Props = $props();

  let status = $state<api.DriveStatus | null>(null);
  let format = $state<"docx" | "googleDoc" | "pdf">("docx");
  let folder = $state("Letters");
  let busy = $state(false);
  let done = $state<{ name: string; link?: string } | null>(null);
  let clientId = $state("");

  const stem = $derived(session.name.replace(/\.[^.]+$/, ""));
  const what = $derived(format === "pdf" ? `a PDF of “${stem}”` : format === "googleDoc" ? `“${stem}” as a Google Doc (sent as a Word file, which Google converts)` : `“${stem}.docx”`);

  async function refresh() {
    status = await api.driveStatus().catch(() => ({ folders: [], canUpload: false, signedIn: false }));
  }

  onMount(() => {
    clientId = session.settings.driveClientId ?? "";
    refresh();
  });

  async function saveToFolder(base: string) {
    const suggested = `${base}\\${folder ? `${folder}\\` : ""}${session.name}`;
    const path = await api.chooseSaveLocationWithFilters(suggested, saveFilters(session.kind));
    if (!path) return;
    const chosen = formatForPath(path);
    try {
      if (chosen?.id === "pdf") await session.exportPdf(path);
      else await api.exportCopy(session.id, session.model(), path);
      session.say(`Saved a copy in the Google Drive folder. Google Drive for desktop uploads it. This document is still “${session.name}”.`);
      onClose();
    } catch (problem) {
      session.say(errorText(problem));
    }
  }

  async function saveClientId() {
    await session.setSettings({ ...session.settings, driveClientId: clientId.trim() });
    await refresh();
  }

  async function signIn() {
    busy = true;
    try {
      await api.driveSignIn();
    } catch (problem) {
      session.say(errorText(problem));
    }
    busy = false;
    await refresh();
  }

  async function upload() {
    busy = true;
    try {
      const uploaded = await api.driveUpload(session.id, session.model(), stem, format, folder.trim() || null);
      done = { name: uploaded.name, link: uploaded.webViewLink };
    } catch (problem) {
      session.say(errorText(problem));
    }
    busy = false;
  }
</script>

<div class="glass sheet" role="dialog" aria-label="Save to Google Drive" style:top="{top + 8}px">
  <h2>Save to Google Drive</h2>
  {#if !status}
    <p class="hint">Looking for Google Drive…</p>
  {:else}
    <section>
      <h3>Google Drive Folder</h3>
      {#if status.folders.length}
        <p class="hint">Google Drive for desktop is on this computer. Letters saves a copy in its folder and Google’s app uploads it. A native Google Doc needs the direct upload below.</p>
        {#each status.folders as base (base)}
          <button class="button" title={tooltipFor("wSaveToDrive")} onclick={() => saveToFolder(base)}>Save to {base}…</button>
        {/each}
      {:else}
        <p class="hint">Google Drive for desktop isn’t on this computer, so there is no Drive folder to save into.</p>
      {/if}
    </section>

    <section>
      <h3>Upload Directly</h3>
      {#if !status.canUpload}
        <p class="hint">Direct upload needs a Google OAuth client ID for desktop apps, which the app’s publisher registers with Google (see Google’s guide, “OAuth 2.0 for iOS & Desktop Apps”). Letters doesn’t ship one.</p>
        <label class="setting">
          <span class="caption">Client ID</span>
          <input class="field" bind:value={clientId} placeholder="….apps.googleusercontent.com" />
        </label>
        <button class="button" disabled={!clientId.trim()} onclick={saveClientId}>Use This Client ID</button>
        <button class="link" onclick={() => api.openLink("https://developers.google.com/identity/protocols/oauth2/native-app")}>How to get a client ID</button>
      {:else if !status.signedIn}
        <p class="hint">Sign in with Google in your browser. Letters asks only to see files it creates itself.</p>
        <button class="button primary" disabled={busy} onclick={signIn}>{busy ? "Waiting for the browser…" : "Sign In with Google…"}</button>
      {:else if done}
        <p>Uploaded “{done.name}”.</p>
        {#if done.link}<button class="button" onclick={() => done?.link && api.openLink(done.link)}>Open in Browser</button>{/if}
      {:else}
        <div class="segments" role="radiogroup" aria-label="Upload as">
          {#each [["docx", "Word File"], ["googleDoc", "Google Doc"], ["pdf", "PDF"]] as const as [value, label] (value)}
            <button class="segment" class:selected={format === value} role="radio" aria-checked={format === value} onclick={() => (format = value)}>{label}</button>
          {/each}
        </div>
        <label class="setting">
          <span class="caption">Folder in My Drive</span>
          <input class="field" bind:value={folder} placeholder="Leave empty for My Drive" />
        </label>
        <p class="hint confirm">Sends {what} from this computer to Google Drive{folder.trim() ? `, folder “${folder.trim()}”` : ""}, in your Google account. Nothing else is sent.</p>
        <div class="buttons">
          <button class="button" onclick={() => api.driveSignOut().then(refresh)}>Sign Out</button>
          <span class="spacer"></span>
          <button class="button primary" disabled={busy} onclick={upload}>{busy ? "Uploading…" : "Upload"}</button>
        </div>
      {/if}
    </section>
  {/if}
  <div class="buttons">
    <span class="spacer"></span>
    <button class="button" onclick={onClose}>Done</button>
  </div>
</div>

<style>
  .sheet {
    position: absolute;
    left: 50%;
    z-index: 4;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    width: min(28rem, calc(100% - 2rem));
    padding: 0.875rem 1rem;
    transform: translateX(-50%);
  }

  h2,
  h3 {
    margin: 0;
    font: var(--headline);
  }

  h3 {
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
  }

  section {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.5rem;
  }

  .setting {
    width: 100%;
  }

  .buttons {
    display: flex;
    width: 100%;
    gap: 0.5rem;
  }

  .spacer {
    flex: 1;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-label);
    font: var(--subheadline);
  }

  .segments {
    display: flex;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segment {
    padding: 0 0.625rem;
    height: 1.5rem;
    border: 0;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 500;
  }

  .segment.selected {
    background: var(--segment-selected);
    box-shadow: var(--shadow-segment);
    color: var(--label);
  }
</style>
