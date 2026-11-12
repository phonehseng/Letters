<script lang="ts">
  // the Settings window: one place for every choice, in sections listed
  // on the leading side, as in a mac's Settings. every change is saved at
  // once; there is no Save button to forget. settings that had a home
  // before (typing, Google Drive, the name on comments) are edited
  // through that home, so nothing is stored twice.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { openFontLibrary } from "../fonts/openFontLibrary";
  import TypingToggles from "../writer/TypingToggles.svelte";
  import { saveWriterSettings, writerSettings, type WriterSettings } from "../writer/writerApi";
  import AccountsSection from "./AccountsSection.svelte";
  import AiSection from "./AiSection.svelte";
  import * as api from "./settingsApi";
  import { OCR_LANGUAGES, privacyItems, ZOOM_CHOICES, type AppSettings } from "./settingsModel";

  const SECTIONS = ["General", "Typing", "Files", "Fonts", "Scanning", "Accounts", "AI Tools", "Privacy"] as const;
  type Section = (typeof SECTIONS)[number];

  let section = $state<Section>("General");
  let view = $state<api.SettingsView | null>(null);
  let settings = $state<AppSettings | null>(null);
  let typing = $state<WriterSettings | null>(null);
  let author = $state("");
  let message = $state<string | null>(null);

  onMount(() => {
    getCurrentWindow().setTitle("Settings");
    load();
    // opened for a section ("#ai-tools") from a panel's Open Settings
    const asked = SECTIONS.find((each) => location.hash === `#${each.toLowerCase().replace(" ", "-")}`);
    if (asked) section = asked;
  });

  async function load() {
    view = await api.loadSettings();
    settings = view.settings;
    typing = await writerSettings().catch(() => null);
    author = await api.authorName();
  }

  /** saves a change at once; what the shell kept comes back. */
  async function change(update: (draft: AppSettings) => void) {
    if (!settings) return;
    const draft: AppSettings = structuredClone($state.snapshot(settings)) as AppSettings;
    update(draft);
    settings = draft;
    try {
      settings = await api.saveSettings(draft);
    } catch (problem) {
      message = String((problem as { message?: string })?.message ?? problem);
    }
  }

  async function saveAuthor() {
    author = await api.setAuthorName(author);
  }

  async function clearRecovery() {
    const count = await api.clearRecoveryCopies();
    message = count === 0 ? "There were no recovery copies." : `Deleted ${count} recovery ${count === 1 ? "copy" : "copies"}.`;
    view = await api.loadSettings();
  }

  async function clearRecents() {
    await api.clearRecents();
    message = "Recents is empty. Your files weren’t touched.";
  }

  function onSectionKey(event: KeyboardEvent, index: number) {
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const nextIndex = (index + step + SECTIONS.length) % SECTIONS.length;
    section = SECTIONS[nextIndex];
    (document.getElementById(`section-${nextIndex}`) as HTMLElement | null)?.focus();
  }
</script>

<div class="settings-window">
  <div class="sections" role="tablist" aria-orientation="vertical" aria-label="Settings sections">
    {#each SECTIONS as each, index (each)}
      <button
        id="section-{index}"
        class="section"
        class:selected={section === each}
        role="tab"
        aria-selected={section === each}
        tabindex={section === each ? 0 : -1}
        onclick={() => (section = each)}
        onkeydown={(event) => onSectionKey(event, index)}>{each}</button
      >
    {/each}
  </div>

  <div class="main" role="tabpanel" aria-label={section}>
    <h1>{section}</h1>
    {#if message}
      <p class="message" role="status">{message} <button class="link" onclick={() => (message = null)}>OK</button></p>
    {/if}

    {#if !settings || !view}
      <p class="hint">Loading…</p>
    {:else if section === "General"}
      <label class="setting">
        <span class="caption">Name on Comments</span>
        <input class="field" bind:value={author} onchange={saveAuthor} />
        <span class="hint">Other readers see this name on your comments, marks and tracked changes. Leave it empty to use the computer’s user name.</span>
      </label>
      <label class="setting">
        <span class="caption">Zoom When a PDF Opens</span>
        <select class="field" value={settings.general.defaultZoom} onchange={(event) => change((draft) => (draft.general.defaultZoom = event.currentTarget.value))}>
          {#each ZOOM_CHOICES as choice (choice.value)}
            <option value={choice.value}>{choice.label}</option>
          {/each}
        </select>
      </label>
      <label class="check">
        <input type="checkbox" checked={settings.general.openInNewWindows} onchange={(event) => change((draft) => (draft.general.openInNewWindows = event.currentTarget.checked))} />
        Open documents from Recents in new windows
      </label>
    {:else if section === "Typing"}
      {#if typing}
        <p class="hint">These are the same four switches as in the writer’s status bar.</p>
        <TypingToggles settings={typing} onChange={async (next) => (typing = await saveWriterSettings(next))} />
      {/if}
    {:else if section === "Files"}
      <div class="setting">
        <span class="caption">New Documents Are Saved As</span>
        <div class="row">
          {#each [["docx", "Word (.docx)"], ["odt", "OpenDocument (.odt)"]] as const as [value, label] (value)}
            <label class="check"><input type="radio" name="document-format" checked={settings.files.document === value} onchange={() => change((draft) => (draft.files.document = value))} /> {label}</label>
          {/each}
        </div>
      </div>
      <div class="setting">
        <span class="caption">A PDF Exported as Word Starts As</span>
        <div class="row">
          {#each [["exactMatch", "Exact Match"], ["easyToEdit", "Easy to Edit"]] as const as [value, label] (value)}
            <label class="check"><input type="radio" name="word-style" checked={settings.files.wordExportStyle === value} onchange={() => change((draft) => (draft.files.wordExportStyle = value))} /> {label}</label>
          {/each}
        </div>
      </div>
      <div class="setting">
        <span class="caption">Pages Exported as Pictures Start As</span>
        <div class="row">
          {#each [["png", "PNG"], ["jpeg", "JPEG"], ["tiff", "TIFF"]] as const as [value, label] (value)}
            <label class="check"><input type="radio" name="picture-format" checked={settings.files.picture === value} onchange={() => change((draft) => (draft.files.picture = value))} /> {label}</label>
          {/each}
        </div>
      </div>
      <div class="setting">
        <span class="caption">Recovery Copies</span>
        <span class="hint">While a document has unsaved changes, Letters keeps a copy every few seconds in {view.recoveryFolder}. There {view.recoveryCopies === 1 ? "is 1 copy" : `are ${view.recoveryCopies} copies`} now.</span>
        <button class="button" onclick={clearRecovery}>Delete Recovery Copies</button>
      </div>
      <div class="setting">
        <span class="caption">Recents</span>
        <button class="button" onclick={clearRecents}>Clear Recents</button>
      </div>
    {:else if section === "Fonts"}
      <p class="hint">Fonts on this computer, fonts you added, and free fonts you can download are in the Font Library.</p>
      <button class="button" onclick={() => openFontLibrary()}>Open Font Library…</button>
    {:else if section === "Scanning"}
      <label class="setting">
        <span class="caption">Language of Scanned Text</span>
        <select class="field" value={settings.scanning.ocrLanguage} onchange={(event) => change((draft) => (draft.scanning.ocrLanguage = event.currentTarget.value))}>
          {#each OCR_LANGUAGES as language (language.value)}
            <option value={language.value}>{language.label}</option>
          {/each}
        </select>
        <span class="hint">Text recognition reads scans and photos best in the language they are written in.</span>
      </label>
    {:else if section === "Accounts"}
      <AccountsSection />
    {:else if section === "AI Tools"}
      <AiSection {settings} onChange={change} />
    {:else if section === "Privacy"}
      <p>Here is everything that can leave this computer, and when. Each one is off until you use it.</p>
      <dl class="privacy">
        {#each privacyItems() as item (item.what)}
          <dt>{item.what}</dt>
          <dd>{item.when}</dd>
        {/each}
      </dl>
      <div class="setting">
        <span class="caption">Default App</span>
        <span class="hint">{view.defaultAppsExplanation}</span>
        {#if view.platform === "windows"}
          <button class="button" onclick={() => api.openDefaultAppsSettings().catch((problem) => (message = String(problem?.message ?? problem)))}>Open Default Apps Settings…</button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .settings-window {
    display: flex;
    height: 100%;
    background: var(--window);
  }

  .sections {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    width: 11rem;
    padding: 1rem 0.5rem;
    border-right: 0.5px solid var(--separator);
    background: var(--glass);
  }

  .section {
    height: 1.75rem;
    padding: 0 0.625rem;
    border: 0;
    border-radius: var(--radius-control);
    background: transparent;
    font: var(--body);
    text-align: left;
  }

  .section:hover {
    background: var(--control-hover);
  }

  .section.selected {
    background: var(--accent);
    color: var(--on-accent);
  }

  .main {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 1rem;
    min-width: 0;
    overflow-y: auto;
    padding: 1.25rem 1.5rem 2rem;
  }

  h1 {
    margin: 0;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 0.375rem;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
  }

  .setting {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.375rem;
  }

  .setting .field {
    width: min(22rem, 100%);
  }

  .message {
    margin: 0;
    padding: 0.5rem 0.75rem;
    border-radius: var(--radius-control);
    background: var(--control);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-label);
  }

  .privacy dt {
    font: var(--headline);
  }

  .privacy dd {
    margin: 0.125rem 0 0.75rem;
  }

  @media (max-width: 36rem) {
    .settings-window {
      flex-direction: column;
    }

    .sections {
      flex-direction: row;
      flex-wrap: wrap;
      width: auto;
      border-right: 0;
      border-bottom: 0.5px solid var(--separator);
    }
  }
</style>
