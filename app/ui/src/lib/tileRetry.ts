// a tile can fail when it was skipped during fast scrolling (the engine
// drops the oldest requests when too many wait). if the tile is still on
// screen it is simply asked for again, a few times at most.

export const MAX_TILE_ATTEMPTS = 4;

/** an `onerror` handler for tile images, used by pages and thumbnails alike. */
export function retryTile(event: Event) {
  const image = event.currentTarget as HTMLImageElement;
  // a tile that has left the screen (or whose document was closed) is not needed
  if (!image.isConnected) return;
  const attempts = Number(image.dataset.attempts ?? 0);
  if (attempts >= MAX_TILE_ATTEMPTS) return;
  image.dataset.attempts = String(attempts + 1);
  const address = image.src.split("#")[0];
  // the part after # is not sent anywhere: it only makes the address
  // look new so the image is loaded again
  setTimeout(
    () => {
      if (image.isConnected) image.src = `${address}#${attempts + 1}`;
    },
    250 * (attempts + 1),
  );
}
