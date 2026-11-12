// making fonts that aren't installed on the system drawable in the web
// view: their bytes come from the shell and are registered with the
// FontFace api, for this window only. nothing is installed anywhere.
// system fonts need none of this; css finds them by name.

import { fontsApi, type FamilySummary } from "./fontsApi";

/** a family name as a css font-family value, with the app's font after it. */
export function cssFamily(name: string): string {
  const quoted = `"${name.replace(/["\\]/g, (c) => `\\${c}`)}"`;
  return `${quoted}, -apple-system, "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`;
}

const loading = new Map<string, Promise<boolean>>();
let running = 0;
const waiting: (() => void)[] = [];

/** at most a few faces load at once, so scrolling stays smooth. */
async function slot<T>(work: () => Promise<T>): Promise<T> {
  if (running >= 3) await new Promise<void>((resolve) => waiting.push(resolve));
  running += 1;
  try {
    return await work();
  } finally {
    running -= 1;
    waiting.shift()?.();
  }
}

async function register(family: string, faceKey: string, descriptors: FontFaceDescriptors): Promise<boolean> {
  const bytes = await slot(() => fontsApi.faceBytes(faceKey));
  const face = new FontFace(family, bytes, descriptors);
  await face.load();
  document.fonts.add(face);
  return true;
}

/**
 * makes a family's regular face drawable, for previews in lists. true
 * when the family can now be drawn by name (or always could).
 */
export function ensurePreviewFace(family: FamilySummary): Promise<boolean> {
  if (!family.needsFontFace || !family.regularFace) return Promise.resolve(family.onThisComputer);
  const key = `preview:${family.regularFace}`;
  let promise = loading.get(key);
  if (!promise) {
    promise = register(family.name, family.regularFace, {}).catch(() => false);
    loading.set(key, promise);
  }
  return promise;
}

/** the weight range a variable face covers, as FontFace wants it. */
export function weightDescriptor(weight: number, axes: { tag: string; min: number; max: number }[]): string {
  const range = axes.find((axis) => axis.tag === "wght");
  return range ? `${Math.round(range.min)} ${Math.round(range.max)}` : String(weight);
}

/**
 * makes every style of a family drawable by its name in this window, so
 * the editor can use an added, bundled or downloaded font exactly like
 * an installed one. resolves to true when it worked.
 */
export async function useFontInDocument(name: string): Promise<boolean> {
  const key = `document:${name.toLowerCase()}`;
  let promise = loading.get(key);
  if (!promise) {
    promise = (async () => {
      const details = await fontsApi.details(name);
      if (!details.summary.onThisComputer) return false;
      if (!details.summary.needsFontFace) return true;
      const styles = details.styles.filter((style) => style.face);
      const results = await Promise.all(
        styles.map((style) =>
          register(details.summary.name, style.face!, {
            weight: weightDescriptor(style.weight, details.axes),
            style: style.italic ? "italic" : "normal",
          }).catch(() => false),
        ),
      );
      return results.some(Boolean);
    })().catch(() => false);
    loading.set(key, promise);
  }
  return promise;
}

/** forget a family's faces after it is removed, so a new copy loads fresh. */
export function forgetFaces(name: string) {
  loading.delete(`document:${name.toLowerCase()}`);
}
