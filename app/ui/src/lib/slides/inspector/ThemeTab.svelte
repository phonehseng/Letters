<script lang="ts">
  // the Theme tab: the deck's twelve theme colours and its two fonts. a
  // change goes to the theme itself, so every slide, layout and object
  // that follows the theme follows at once; colours picked by hand stay.
  import { tools } from "../../tools";
  import type { DeckSession } from "../deckSession.svelte";
  import { rgbColor } from "../operations";

  interface Props {
    session: DeckSession;
    onAnnounce: (words: string) => void;
  }

  let { session, onAnnounce }: Props = $props();

  /** the slots in plain words, in the order powerpoint's theme colours dialog lists them */
  const SLOT_NAMES: Record<string, string> = {
    dk1: "Text, dark",
    lt1: "Background, light",
    dk2: "Text 2, dark",
    lt2: "Background 2, light",
    accent1: "Accent 1",
    accent2: "Accent 2",
    accent3: "Accent 3",
    accent4: "Accent 4",
    accent5: "Accent 5",
    accent6: "Accent 6",
    hlink: "Link",
    folHlink: "Visited link",
  };

  const theme = $derived(session.deck.theme);
  let major = $state("");
  let minor = $state("");

  $effect(() => {
    major = theme?.majorFont ?? "";
    minor = theme?.minorFont ?? "";
  });

  async function setColor(slot: string, hex: string) {
    if (!theme) return;
    if (await session.apply({ op: "setThemeColor", theme_id: theme.id, slot, color: rgbColor(hex) })) onAnnounce(`${SLOT_NAMES[slot] ?? slot} changed on every slide.`);
  }

  async function setFonts(event: SubmitEvent) {
    event.preventDefault();
    if (!theme || !major.trim() || !minor.trim()) return;
    if (await session.apply({ op: "setThemeFonts", theme_id: theme.id, major: major.trim(), minor: minor.trim() })) onAnnounce("Theme fonts changed on every slide.");
  }
</script>

{#if theme}
  <section>
    <h3>{theme.name || tools.sTheme.label}</h3>
    <p class="hint">{tools.sTheme.meaning}</p>
  </section>
  <section>
    <h3>Colours</h3>
    <div class="colors">
      {#each theme.colors as color (color.slot)}
        <label class="color" title="{SLOT_NAMES[color.slot] ?? color.slot}: pick a colour">
          <input type="color" value={color.hex.toLowerCase()} onchange={(event) => void setColor(color.slot, (event.currentTarget as HTMLInputElement).value)} />
          <span>{SLOT_NAMES[color.slot] ?? color.slot}</span>
        </label>
      {/each}
    </div>
  </section>
  <form onsubmit={setFonts}>
    <section>
      <h3>Fonts</h3>
      <label class="setting">
        <span class="caption">Headings</span>
        <input class="field" bind:value={major} style:font-family="'{major}', sans-serif" />
      </label>
      <label class="setting">
        <span class="caption">Body</span>
        <input class="field" bind:value={minor} style:font-family="'{minor}', sans-serif" />
      </label>
      <button class="button" type="submit" disabled={major === theme.majorFont && minor === theme.minorFont}>Change Fonts</button>
      <p class="hint">Titles use the heading font and other text the body font, unless a font was picked by hand. A font this computer doesn’t have is drawn with a stand-in, and the Slide tab says which.</p>
    </section>
  </form>
{:else}
  <section><p class="hint">This presentation has no theme to change.</p></section>
{/if}

<style>
  .colors {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.375rem;
  }

  .color {
    display: flex;
    align-items: center;
    gap: 0.375rem;
    font: var(--subheadline);
  }

  .color input {
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    border: 0;
    background: none;
  }
</style>
