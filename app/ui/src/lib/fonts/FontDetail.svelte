<script lang="ts">
  // the detail pane of the Font Library: one family's styles, languages,
  // licence and embedding in plain sentences, where it comes from, and
  // its one action (Download for a library font). a font that isn't
  // downloaded can be previewed by fetching only its regular file.
  import { tools } from "../tools";
  import { cssFamily, forgetFaces, useFontInDocument } from "./fontFaces";
  import { fontsApi, type FamilyDetails, type FamilySummary, type Preview } from "./fontsApi";
  import { sizeWords } from "./fontQuery";
  import { fontStore } from "./fontStore.svelte";
  import DownloadNotice from "./DownloadNotice.svelte";
  import FontSample from "./FontSample.svelte";
  import { detailSample } from "./sampleText";

  interface Props {
    family: FamilySummary | null;
    sample: string;
    writing: string | null;
    size: number;
    onShowFamily: (name: string) => void;
    onAnnounce: (text: string) => void;
  }

  let { family, sample, writing, size, onShowFamily, onAnnounce }: Props = $props();

  const store = fontStore();
  let details = $state<FamilyDetails | null>(null);
  let preview = $state<Preview | null>(null);
  let problem = $state<string | null>(null);
  let asking = $state<null | "download" | "preview">(null);
  let working = $state(false);

  const text = $derived(detailSample(sample, writing));
  const download = $derived(
    family ? store.status?.downloads.find((entry) => entry.family === family.name && !entry.finished) : undefined,
  );
  const downloading = $derived(!!download && !download.error);
  const removable = $derived(!!details && details.summary.sources.some((source) => source === "custom" || source === "library"));

  async function load(name: string) {
    problem = null;
    try {
      const found = await fontsApi.details(name);
      if (family?.name !== name) return;
      details = found;
      preview = null;
      if (!found.summary.onThisComputer && found.previewReady) {
        preview = await fontsApi.preview(name, text, Math.round(size * (4 / 3)));
      }
    } catch (error) {
      problem = String((error as { message?: string }).message ?? error);
    }
  }

  // read the details again when the family, its revision or the sample changes
  $effect(() => {
    void store.revision;
    void text;
    if (family) void load(family.name);
    else details = null;
  });

  // a download that failed says why
  $effect(() => {
    if (download?.error) problem = download.error;
  });

  async function startDownload() {
    if (!family) return;
    try {
      await fontsApi.download(family.name);
      onAnnounce(`Downloading ${family.name}…`);
      await store.refresh();
    } catch (error) {
      problem = String((error as { message?: string }).message ?? error);
    }
  }

  async function fetchPreview() {
    if (!family) return;
    working = true;
    try {
      await fontsApi.fetchPreview(family.name);
      await load(family.name);
    } catch (error) {
      problem = String((error as { message?: string }).message ?? error);
    } finally {
      working = false;
    }
  }

  function ask(action: "download" | "preview") {
    if (store.status?.downloadNoticeAccepted) {
      void (action === "download" ? startDownload() : fetchPreview());
    } else {
      asking = action;
    }
  }

  async function accepted() {
    const action = asking;
    asking = null;
    await fontsApi.acceptDownloadNotice();
    await store.refresh();
    if (action) ask(action);
  }

  async function remove() {
    if (!family) return;
    const name = family.name;
    try {
      await fontsApi.remove(name);
      forgetFaces(name);
      onAnnounce(`${name} was removed from Letters.`);
      await store.refresh();
    } catch (error) {
      problem = String((error as { message?: string }).message ?? error);
    }
  }

  async function toggleFavourite() {
    if (!family) return;
    const favourite = !family.favourite;
    await store.setFavourite(family.name, favourite);
    onAnnounce(favourite ? `${family.name} added to Favourites.` : `${family.name} removed from Favourites.`);
  }

  async function prepareForDocuments() {
    if (!family) return;
    const ok = await useFontInDocument(family.name);
    await store.noteUsed(family.name);
    onAnnounce(ok ? `${family.name} can now be used in documents in this window.` : `${family.name} couldn’t be loaded.`);
  }
</script>

<aside class="detail" aria-label="Font details">
  {#if !family}
    <p class="placeholder">Choose a font to see its styles, languages and license.</p>
  {:else}
    <h3>{family.name}</h3>
    <div class="specimen">
      {#if family.onThisComputer}
        <FontSample {family} {text} size={Math.max(size, 20)} />
      {:else if preview}
        <svg
          class="preview"
          viewBox="0 0 {Math.max(1, preview.width)} {Math.max(1, preview.height)}"
          role="img"
          aria-label="{text} in {family.name}"
          style:height="{preview.height}px"
        >
          <path d={preview.path} />
        </svg>
      {:else}
        <p class="hint">{family.name} isn’t on this computer yet. Show Preview downloads only its regular style to draw this.</p>
      {/if}
    </div>

    <div class="actions">
      {#if !family.onThisComputer}
        {#if downloading && download}
          <progress max={download.progress.total} value={download.progress.done} aria-label="Download progress"></progress>
          <span class="hint">{sizeWords(download.progress.done)} of {sizeWords(download.progress.total)}</span>
          <button class="button" onclick={() => family && fontsApi.cancelDownload(family.name)}>Cancel</button>
        {:else}
          <button class="button primary" title={tools.downloadFont.tooltip} onclick={() => ask("download")}>
            Download{family.downloadBytes ? ` (${sizeWords(family.downloadBytes)})` : ""}
          </button>
          {#if !preview}
            <button class="button" disabled={working} onclick={() => ask("preview")}>{working ? "Getting Preview…" : "Show Preview"}</button>
          {/if}
        {/if}
      {:else}
        <button class="button" title={tools.useFont.tooltip} onclick={prepareForDocuments}>{tools.useFont.label}</button>
        {#if removable}
          <button class="button" title={tools.removeFont.tooltip} onclick={remove}>{tools.removeFont.label}</button>
        {/if}
      {/if}
      <button class="button" aria-pressed={family.favourite} onclick={toggleFavourite}>
        {family.favourite ? "Remove from Favourites" : "Add to Favourites"}
      </button>
    </div>

    {#if problem}
      <p class="problem" role="alert">{problem}</p>
    {/if}

    {#if details}
      <section>
        <h4>Styles</h4>
        <ul class="styles">
          {#each details.styles as style (style.name + style.weight + style.italic)}
            <li
              style:font-family={family.onThisComputer ? cssFamily(family.name) : undefined}
              style:font-weight={style.weight}
              style:font-style={style.italic ? "italic" : "normal"}
            >
              {style.name}
            </li>
          {/each}
        </ul>
      </section>

      {#if details.writing.length}
        <section>
          <h4>Languages</h4>
          <p>{details.writing.join(", ")}</p>
        </section>
      {/if}

      <section>
        <h4>License and Embedding</h4>
        {#if details.license}<p><strong>{details.license}.</strong> Free to use, share and embed.</p>{/if}
        <p>{details.embeddingSentence}</p>
      </section>

      <section>
        <h4>Where It Comes From</h4>
        <p>{details.sourceSentence}</p>
        {#if details.designer}<p class="hint">Designed by {details.designer}.</p>{/if}
        {#if details.version || details.glyphCount}
          <p class="hint">
            {#if details.version}Version {details.version}.{/if}
            {#if details.glyphCount}{details.glyphCount.toLocaleString("en-US")} glyphs.{/if}
            {#if details.axes.length}Variable: {details.axes.map((axis) => `${axis.tag} ${axis.min}–${axis.max}`).join(", ")}.{/if}
          </p>
        {/if}
      </section>

      {#if details.similar.length}
        <section>
          <h4>Similar Fonts</h4>
          <ul class="similar">
            {#each details.similar as other (other.family)}
              <li>
                <button class="link" onclick={() => onShowFamily(other.family)}>{other.family}</button>
                {#if other.metricCompatible}<span class="hint">same letter widths</span>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  {/if}
</aside>

{#if asking}
  <DownloadNotice host={store.status?.downloadHost ?? "raw.githubusercontent.com"} onCancel={() => (asking = null)} onContinue={accepted} />
{/if}

<style>
  .detail {
    flex: 1;
    min-width: 16rem;
    max-width: 26rem;
    padding: 1rem;
    overflow-y: auto;
    border-left: 0.5px solid var(--separator);
  }

  .placeholder {
    margin-top: 3rem;
    color: var(--label-secondary);
    text-align: center;
  }

  h3 {
    margin: 0 0 0.5rem;
    font: var(--title);
    letter-spacing: var(--title-spacing);
  }

  h4 {
    margin: 0 0 0.25rem;
    color: var(--label-secondary);
    font: var(--subheadline);
    font-weight: 600;
    letter-spacing: var(--subheadline-spacing);
  }

  section {
    margin-top: 1rem;
  }

  section p {
    margin: 0 0 0.25rem;
    line-height: 1.125rem;
  }

  .specimen {
    min-height: 3rem;
    padding: 0.5rem 0;
    overflow: hidden;
  }

  .specimen :global(.sample) {
    white-space: normal;
  }

  .preview {
    display: block;
    max-width: 100%;
    fill: var(--label);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  progress {
    flex: 1;
    min-width: 6rem;
    accent-color: var(--accent);
  }

  .problem {
    margin: 0.5rem 0 0;
    color: var(--danger);
  }

  .styles,
  .similar {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .styles li {
    padding: 0.125rem 0;
    font-size: 1rem;
  }

  .similar li {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.125rem 0;
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-label);
    font: var(--body);
  }

  .link:hover {
    text-decoration: underline;
  }

  @media (max-width: 46rem) {
    .detail {
      max-width: none;
      border-left: 0;
      border-top: 0.5px solid var(--separator);
    }
  }
</style>
