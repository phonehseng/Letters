<script lang="ts">
  // a line of text drawn in a family's own face. installed fonts are
  // drawn by name at once; added, bundled and downloaded ones are given
  // to the web view first (fontFaces.ts), which only happens for rows on
  // screen because the lists are virtual. fonts not downloaded yet are
  // drawn in the app's font, quietly, since there is nothing to draw with.
  import { cssFamily, ensurePreviewFace } from "./fontFaces";
  import type { FamilySummary } from "./fontsApi";

  interface Props {
    family: FamilySummary | null;
    /** the name to draw with when there is no summary (a missing font) */
    name?: string;
    text: string;
    /** in points */
    size?: number;
  }

  let { family, name = "", text, size = 13 }: Props = $props();

  let ready = $state(false);

  $effect(() => {
    const current = family;
    ready = !!current && current.onThisComputer && !current.needsFontFace;
    if (current?.needsFontFace) {
      void ensurePreviewFace(current).then((ok) => {
        if (family === current) ready = ok;
      });
    }
  });

  const style = $derived(
    `font-size: ${size}pt;` + (ready && family ? ` font-family: ${cssFamily(family.name)};` : ""),
  );
</script>

<span class="sample" class:unavailable={!ready} {style} title={family?.name ?? name}>{text}</span>

<style>
  .sample {
    display: block;
    overflow: hidden;
    line-height: 1.3;
    text-overflow: ellipsis;
    white-space: nowrap;
    letter-spacing: normal;
  }

  .unavailable {
    color: var(--label-secondary);
  }
</style>
