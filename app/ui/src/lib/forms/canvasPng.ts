// turning a drawn canvas into the png bytes the shell keeps. the canvas
// is cut down to what was drawn first, so a small signature isn't kept
// with a wide empty margin around it.

import type { Rect } from "../pageTools/pagePoints";

/** the png of part of a canvas, in canvas pixels. */
export async function canvasPng(canvas: HTMLCanvasElement, crop?: Rect): Promise<Uint8Array> {
  const area = crop ?? { x: 0, y: 0, width: canvas.width, height: canvas.height };
  const out = document.createElement("canvas");
  out.width = Math.max(1, Math.round(area.width));
  out.height = Math.max(1, Math.round(area.height));
  out.getContext("2d")?.drawImage(canvas, area.x, area.y, area.width, area.height, 0, 0, out.width, out.height);
  const blob = await new Promise<Blob | null>((resolve) => out.toBlob(resolve, "image/png"));
  if (!blob) throw new Error("The signature couldn’t be made into a picture. Try again.");
  return new Uint8Array(await blob.arrayBuffer());
}

/** a picture from a data: address, once it has loaded. */
export function loadImage(source: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("Letters can’t read this picture. Choose a PNG or JPEG picture of your signature."));
    image.src = source;
  });
}
