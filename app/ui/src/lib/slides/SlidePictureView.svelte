<script lang="ts">
  // one slide's picture at a size: the tiles the app's pdf renderer draws
  // of the slide's one-page pdf (see shell-core slides/pictures.rs). a new
  // revision fetches new tiles; the old ones stay until the new ones have
  // loaded, so an edit never flashes the slide white.
  import { tileUrl } from "../api";
  import { retryTile } from "../tileRetry";
  import { tilesFor } from "./geometry";
  import type { SlidePicture } from "./slidesApi";

  interface Props {
    picture: SlidePicture | null;
    /** css pixels per point */
    scale: number;
    label?: string;
    /** told the revision on screen whenever it changes */
    onShown?: (revision: number) => void;
  }

  let { picture, scale, label = "", onShown }: Props = $props();

  const ratio = typeof window === "undefined" ? 1 : window.devicePixelRatio || 1;
  // the revision on screen: it moves on only once every tile of the next one has loaded
  let shown = $state<SlidePicture | null>(null);
  let loading = $state<SlidePicture | null>(null);
  let loaded = 0;

  const grid = $derived(picture ? tilesFor(picture, scale, ratio) : null);
  const shownGrid = $derived(shown ? tilesFor(shown, scale, ratio) : null);

  $effect(() => {
    if (!picture) return;
    if (!shown) {
      shown = picture;
    } else if (picture.revision !== shown.revision && picture.revision !== loading?.revision) {
      // another slide, a redrawn one, or the one with text left out for
      // typing: it replaces the old picture once it has loaded
      loaded = 0;
      loading = picture;
    } else if (picture.revision === shown.revision) {
      loading = null;
    }
  });

  $effect(() => {
    if (shown) onShown?.(shown.revision);
  });

  function onLoaded() {
    loaded += 1;
    if (loading && grid && loaded >= grid.tiles.length) {
      shown = loading;
      loading = null;
    }
  }
</script>

<div class="picture" role={label ? "img" : undefined} aria-label={label || undefined}>
  {#if shown && shownGrid}
    {#each shownGrid.tiles as tile (`${shown.revision}/${shownGrid.scaleMilli}/${tile.column}/${tile.row}`)}
      <img
        src={tileUrl(shown.document, 0, shownGrid.scaleMilli, tile.column, tile.row, shown.revision)}
        alt=""
        draggable="false"
        onerror={retryTile}
        style:left="{tile.left}px"
        style:top="{tile.top}px"
        style:width="{tile.width}px"
        style:height="{tile.height}px"
      />
    {/each}
  {/if}
  {#if loading && grid}
    <!-- the next revision loads out of sight, then takes the old one's place -->
    {#each grid.tiles as tile (`${loading.revision}/${grid.scaleMilli}/${tile.column}/${tile.row}`)}
      <img
        class="next"
        src={tileUrl(loading.document, 0, grid.scaleMilli, tile.column, tile.row, loading.revision)}
        alt=""
        draggable="false"
        onload={onLoaded}
        onerror={retryTile}
        style:left="{tile.left}px"
        style:top="{tile.top}px"
        style:width="{tile.width}px"
        style:height="{tile.height}px"
      />
    {/each}
  {/if}
</div>

<style>
  .picture {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: #ffffff;
  }

  img {
    position: absolute;
    user-select: none;
    pointer-events: none;
  }

  .next {
    opacity: 0;
  }
</style>
