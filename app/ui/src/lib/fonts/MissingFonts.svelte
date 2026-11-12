<script lang="ts">
  // the Missing Fonts view: each font the open document asks for that
  // isn't on this computer, the stand-in Letters uses and why, and one
  // click to download it or put the stand-in in the document.
  import { fontsApi, type MissingFont } from "./fontsApi";
  import { fontStore } from "./fontStore.svelte";
  import DownloadNotice from "./DownloadNotice.svelte";
  import FontSample from "./FontSample.svelte";
  import { missingActions, missingSummary, missingTitle, type MissingAction } from "./missingFonts";

  interface Props {
    fonts: MissingFont[];
    onReplaceFont?: (from: string, to: string) => void;
    onAnnounce: (text: string) => void;
    onShowFamily: (name: string) => void;
  }

  let { fonts, onReplaceFont, onAnnounce, onShowFamily }: Props = $props();

  const store = fontStore();
  let waiting = $state<MissingAction | null>(null);
  let problem = $state<string | null>(null);

  async function run(action: MissingAction) {
    problem = null;
    if (action.kind === "replace") {
      onReplaceFont?.(action.from, action.to);
      onAnnounce(`${action.from} was replaced with ${action.to}.`);
      return;
    }
    if (!store.status?.downloadNoticeAccepted) {
      waiting = action;
      return;
    }
    try {
      await fontsApi.download(action.family);
      onAnnounce(`Downloading ${action.family}…`);
      await store.refresh();
    } catch (error) {
      problem = String((error as { message?: string }).message ?? error);
    }
  }

  async function accepted() {
    const action = waiting;
    waiting = null;
    await fontsApi.acceptDownloadNotice();
    await store.refresh();
    if (action) await run(action);
  }

  function progressOf(family: string): string | null {
    const download = store.status?.downloads.find((entry) => entry.family === family && !entry.finished && !entry.error);
    if (!download) return null;
    const percent = download.progress.total ? Math.round((100 * download.progress.done) / download.progress.total) : 0;
    return `Downloading… ${percent}%`;
  }
</script>

<div class="missing">
  <h3>{missingTitle(fonts.length)}</h3>
  <p class="summary">{missingSummary(fonts)}</p>
  {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
  <ul>
    {#each fonts as font (font.name)}
      {@const actions = missingActions(font, !!onReplaceFont)}
      <li>
        <div class="names">
          <span class="asked">{font.name}</span>
          <span class="arrow" aria-hidden="true">→</span>
          <button class="link" onclick={() => onShowFamily(font.substitution.used)}>
            <FontSample family={store.family(font.substitution.used) ?? null} name={font.substitution.used} text={font.substitution.used} size={13} />
          </button>
          {#if font.substitution.metricCompatible}<span class="badge">Same Widths</span>{/if}
        </div>
        <p class="sentence">{font.substitution.sentence}</p>
        <div class="actions">
          {#each actions as action, index (action.label)}
            {@const progress = action.kind === "replace" ? null : progressOf(action.family)}
            {#if progress}
              <span class="hint">{progress}</span>
            {:else}
              <button class="button" class:primary={index === 0 && fonts.indexOf(font) === 0} onclick={() => run(action)}>{action.label}</button>
            {/if}
          {/each}
        </div>
      </li>
    {/each}
  </ul>
</div>

{#if waiting}
  <DownloadNotice host={store.status?.downloadHost ?? "raw.githubusercontent.com"} onCancel={() => (waiting = null)} onContinue={accepted} />
{/if}

<style>
  .missing {
    flex: 1;
    overflow-y: auto;
    padding: 0.5rem 1rem 1rem;
    border-top: 0.5px solid var(--separator);
  }

  h3 {
    margin: 0.5rem 0 0.25rem;
    font: var(--headline);
  }

  .summary {
    margin: 0 0 0.75rem;
    color: var(--label-secondary);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    padding: 0.75rem 0;
    border-bottom: 0.5px solid var(--separator);
  }

  .names {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
  }

  .asked {
    font: var(--headline);
  }

  .arrow {
    color: var(--label-secondary);
  }

  .sentence {
    margin: 0.25rem 0 0.5rem;
    line-height: 1.125rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .badge {
    padding: 0 0.375rem;
    border-radius: 0.5rem;
    background: var(--control);
    font: var(--footnote);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-label);
  }

  .problem {
    color: var(--danger);
  }
</style>
