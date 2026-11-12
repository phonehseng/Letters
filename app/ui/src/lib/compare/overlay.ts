// the overlay view: the engine's overlay picture of each page pair (shared
// ink grey, removed red, added green), with a slider that fades from the
// old page through the overlay to the new page, and a Flash button that
// alternates the two versions.

/** how visible each layer is, from 0 to 1. */
export interface Layers {
  old: number;
  overlay: number;
  next: number;
}

/**
 * the layers for a slider position from 0 (the old page) to 100 (the new
 * page). the middle shows the overlay picture alone, so both ends and the
 * middle are each one clear picture.
 */
export function layersAt(position: number): Layers {
  const t = Math.min(100, Math.max(0, position)) / 100;
  return {
    old: Math.max(0, 1 - 2 * t),
    overlay: 1,
    next: Math.max(0, 2 * t - 1),
  };
}

/** what the slider's position is said as, for screen readers. */
export function sliderText(position: number): string {
  if (position <= 0) return "Old version";
  if (position >= 100) return "New version";
  if (position === 50) return "Both versions, changes in colour";
  return position < 50 ? `Mostly the old version (${position}%)` : `Mostly the new version (${position}%)`;
}

/** how long each version shows while flashing. */
export const FLASH_MS = 700;

/**
 * the next slider position while flashing: old, new, old... starting from
 * whichever end is further away so the first step is visible.
 */
export function flashStep(position: number): number {
  return position >= 50 ? 0 : 100;
}

/**
 * what the Flash button does. with reduced motion nothing moves by
 * itself: each press shows the other version once.
 */
export function flashMode(reducedMotion: boolean): "auto" | "step" {
  return reducedMotion ? "step" : "auto";
}

/** the scale to ask the engine for, in pixels per point: sharp on this screen, never huge. */
export function pictureScale(zoom: number, pixelRatio: number): number {
  const scale = zoom * pixelRatio;
  // whole tenths keep the pictures cached across tiny zoom differences
  return Math.min(3, Math.max(0.25, Math.round(scale * 10) / 10));
}
