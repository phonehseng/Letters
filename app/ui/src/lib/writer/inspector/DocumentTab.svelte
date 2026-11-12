<script lang="ts">
  // the Document tab: the page (size, orientation, margins, columns), the
  // header and footer, the MLA and APA presets, and the four typing
  // switches (the same ones as in the status bar).
  import { tools, tooltipFor } from "../../tools";
  import * as edit from "../editing";
  import { PAGE_SIZES } from "../editing";
  import { lengthToTwips, lengthValue, type LengthUnit } from "../units";
  import TypingToggles from "../TypingToggles.svelte";
  import { gate } from "../strictMode";
  import type { WriterSession } from "../writerSession.svelte";

  interface Props {
    session: WriterSession;
    unit: LengthUnit;
  }

  let { session, unit }: Props = $props();

  const section = $derived(session.meta.sections?.[0]);
  const page = $derived(section?.page);
  const sizeKey = $derived.by(() => {
    if (!page) return "letter";
    const short = Math.min(page.width, page.height);
    const long = Math.max(page.width, page.height);
    const found = Object.entries(PAGE_SIZES).find(([, size]) => Math.abs(size.width - short) < 30 && Math.abs(size.height - long) < 30);
    return found ? found[0] : "custom";
  });
  const landscape = $derived(!!page && page.width > page.height);
  const hasHeader = $derived(!!section?.headers?.default || !!section?.headers?.first);
  const hasFooter = $derived(!!section?.footers?.default || !!section?.footers?.first);

  // the preset's optional details; none are needed for the formatting itself
  let name = $state("");
  let instructor = $state("");
  let course = $state("");
  let title = $state("");
  let addHeading = $state(false);
  let presetOpen = $state<"mla" | "apa" | null>(null);

  const run = (make: Parameters<WriterSession["run"]>[0]) => session.run(make, false);
  const mode = $derived(session.settings.mode === "strict" ? "strict" : "free");
  // strict word mode turns off what word would show differently (strictMode.ts)
  const columnsGate = $derived(gate("wColumns", mode));

  function setMargin(side: "top" | "bottom" | "left" | "right", value: string) {
    const number = Number.parseFloat(value);
    if (Number.isFinite(number) && number >= 0) run((s) => edit.setMargin(s, side, lengthToTwips(number, unit)));
  }

  function applyPreset(preset: "mla" | "apa") {
    const date = new Date().toLocaleDateString(preset === "mla" ? "en-GB" : "en-US", { day: "numeric", month: "long", year: "numeric" });
    session.applyPreset(preset, { name, instructor, course, title, date, addHeading });
  }
</script>

<section aria-labelledby="w-mode">
  <h3 id="w-mode">Editing Mode</h3>
  <div class="segments" role="radiogroup" aria-label="Editing mode" title={tooltipFor("wStrictMode")}>
    <button class="segment" class:selected={mode === "free"} role="radio" aria-checked={mode === "free"} onclick={() => session.setSettings({ ...session.settings, mode: "free" })}>Free</button>
    <button class="segment" class:selected={mode === "strict"} role="radio" aria-checked={mode === "strict"} onclick={() => session.setSettings({ ...session.settings, mode: "strict" })}>Strict Word</button>
  </div>
  <p class="hint">{mode === "strict" ? "Only what Microsoft Word keeps exactly. With Word installed, Word itself lays out the pages you see." : "Every Letters feature. Page breaks are estimated on screen."}</p>
</section>

<section aria-labelledby="w-page">
  <h3 id="w-page">Page</h3>
  <div class="setting-row">
    <label class="setting">
      <span class="caption" title={tooltipFor("wPageSize")}>{tools.wPageSize.label}</span>
      <select class="field" value={sizeKey} onchange={(event) => run((s) => edit.setPageSize(s, event.currentTarget.value as keyof typeof PAGE_SIZES))}>
        {#each Object.entries(PAGE_SIZES) as [key, size] (key)}<option value={key}>{size.label}</option>{/each}
        {#if sizeKey === "custom"}<option value="custom" disabled>Custom</option>{/if}
      </select>
    </label>
    <label class="setting">
      <span class="caption" title={tooltipFor("wOrientation")}>{tools.wOrientation.label}</span>
      <select class="field" value={landscape ? "landscape" : "portrait"} onchange={(event) => run((s) => edit.setOrientation(s, event.currentTarget.value as "portrait" | "landscape"))}>
        <option value="portrait">Portrait</option>
        <option value="landscape">Landscape</option>
      </select>
    </label>
  </div>
  <div class="setting">
    <span class="caption" title={tooltipFor("wMargins")}>{tools.wMargins.label} ({unit})</span>
    <div class="setting-row">
      {#each ["top", "bottom", "left", "right"] as const as side (side)}
        <label class="setting">
          <span class="caption">{side.charAt(0).toUpperCase() + side.slice(1)}</span>
          <input class="field" type="number" min="0" step="0.1" value={lengthValue(page?.margins[side] ?? 1440, unit)} onchange={(event) => setMargin(side, event.currentTarget.value)} />
        </label>
      {/each}
    </div>
  </div>
  <label class="setting">
    <span class="caption" title={tooltipFor("wColumns")}>{tools.wColumns.label}</span>
    <select class="field" disabled={!columnsGate.enabled} title={columnsGate.reason} value={String(section?.columns?.count ?? 1)} onchange={(event) => run((s) => edit.setColumns(s, Number(event.currentTarget.value)))}>
      <option value="1">One</option>
      <option value="2">Two</option>
      <option value="3">Three</option>
    </select>
  </label>
</section>

<section aria-labelledby="w-header-footer">
  <h3 id="w-header-footer" title={tooltipFor("wHeaderFooter")}>{tools.wHeaderFooter.label}</h3>
  <p class="hint">{tools.wHeaderFooter.meaning}</p>
  <div class="buttons">
    <button class="button" disabled={hasHeader} onclick={() => session.run((s) => edit.addHeaderFooter(s, "header").tr)}>{hasHeader ? "Has a Header" : "Add Header"}</button>
    <button class="button" disabled={hasFooter} onclick={() => session.run((s) => edit.addHeaderFooter(s, "footer").tr)}>{hasFooter ? "Has a Footer" : "Add Footer"}</button>
  </div>
  <label class="check">
    <input type="checkbox" checked={!!section?.differentFirstPage} onchange={(event) => run((s) => edit.setDifferentFirstPage(s, event.currentTarget.checked))} />
    Different First Page
  </label>
  <p class="hint">Click in the header or footer on the page to type in it; Insert › Page Number adds the number.</p>
</section>

<section aria-labelledby="w-presets">
  <h3 id="w-presets">Paper Style</h3>
  <p class="hint">One click sets up the whole document the way the style guide asks. Undo takes it back.</p>
  <div class="preset">
    <div>
      <span class="name">{tools.wPresetMla.label}</span>
      <p class="hint">1 inch margins, Times New Roman 12, double spacing, half-inch first-line indents and “Lastname 1” at the top right.</p>
    </div>
    <button class="button" class:primary={presetOpen === "mla"} title={tooltipFor("wPresetMla")} onclick={() => (presetOpen === "mla" ? applyPreset("mla") : (presetOpen = "mla"))}>
      {presetOpen === "mla" ? "Apply MLA" : "MLA…"}
    </button>
  </div>
  <div class="preset">
    <div>
      <span class="name">{tools.wPresetApa.label}</span>
      <p class="hint">1 inch margins, 12 point, double spacing, half-inch indents, page numbers top right, APA headings and hanging indents under “References”.</p>
    </div>
    <button class="button" class:primary={presetOpen === "apa"} title={tooltipFor("wPresetApa")} onclick={() => (presetOpen === "apa" ? applyPreset("apa") : (presetOpen = "apa"))}>
      {presetOpen === "apa" ? "Apply APA" : "APA…"}
    </button>
  </div>
  {#if presetOpen}
    <div class="details">
      <p class="hint">It changes the whole document. Your name goes in the header{presetOpen === "mla" ? " (MLA uses your last name)" : ""}; the rest is only needed for the {presetOpen === "mla" ? "heading block" : "title page"}.</p>
      <label class="setting"><span class="caption">Your Name</span><input class="field" bind:value={name} /></label>
      <label class="check"><input type="checkbox" bind:checked={addHeading} />Add the {presetOpen === "mla" ? "MLA heading block" : "APA title page"}</label>
      {#if addHeading}
        <label class="setting"><span class="caption">Instructor</span><input class="field" bind:value={instructor} /></label>
        <label class="setting"><span class="caption">Course</span><input class="field" bind:value={course} /></label>
        <label class="setting"><span class="caption">Title</span><input class="field" bind:value={title} /></label>
      {/if}
      <p class="hint">Changes the whole document: margins, the body style, spacing and indents, and the header.</p>
    </div>
  {/if}
</section>

<section aria-labelledby="w-typing">
  <h3 id="w-typing">Typing</h3>
  <TypingToggles settings={session.settings} onChange={(next) => session.setSettings(next)} />
  <p class="hint">The same switches are in the status bar. They apply to every document.</p>
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
    padding: 0.75rem 0.25rem;
    border-top: 0.5px solid var(--separator);
  }

  section:first-child {
    border-top: 0;
  }

  h3 {
    margin: 0;
    font: var(--headline);
  }

  .setting-row {
    grid-template-columns: repeat(auto-fit, minmax(3.5rem, 1fr));
  }

  .buttons {
    display: flex;
    gap: 0.375rem;
  }

  .preset {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .name {
    font: var(--headline);
  }

  .segments {
    display: flex;
    padding: 0.125rem;
    border-radius: calc(var(--radius-small) + 0.125rem);
    background: var(--control);
  }

  .segment {
    flex: 1;
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

  .details {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.5rem;
    border-radius: var(--radius-small);
    background: var(--control);
  }
</style>
