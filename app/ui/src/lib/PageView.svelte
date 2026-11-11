<script lang="ts">
  // one page of the document: a white sheet made of the tiles in view.
  import { tileUrl } from "./api";
  import type { PlacedPage, TileSpot } from "./layout";
  import { retryTile } from "./tileRetry";

  interface Props {
    documentId: number;
    page: PlacedPage;
    /** the page's left edge in the scroll area, in css pixels */
    left: number;
    scale: number;
    tiles: TileSpot[];
  }

  let { documentId, page, left, scale, tiles }: Props = $props();
</script>

<div
  class="page"
  role="img"
  aria-label={`Page ${page.index + 1}`}
  style:top="{page.top}px"
  style:left="{left}px"
  style:width="{page.width}px"
  style:height="{page.height}px"
>
  {#each tiles as tile (`${scale}/${tile.column}/${tile.row}`)}
    <img
      src={tileUrl(documentId, page.index, scale, tile.column, tile.row)}
      alt=""
      draggable="false"
      onerror={retryTile}
      style:left="{tile.left}px"
      style:top="{tile.top}px"
      style:width="{tile.width}px"
      style:height="{tile.height}px"
    />
  {/each}
</div>

<style>
  .page {
    position: absolute;
    overflow: hidden;
    border-radius: 2px;
    background: #ffffff;
    box-shadow: var(--shadow-page);
  }

  .page img {
    position: absolute;
    user-select: none;
  }

  /* night mode flips light and dark, then turns the colours back around
     so blue stays blue. it is only a screen effect. the viewer sets the
     night class on the scroll area around the pages. */
  :global(.night) .page {
    filter: invert(0.92) hue-rotate(180deg);
  }
</style>
