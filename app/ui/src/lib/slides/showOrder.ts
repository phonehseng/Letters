// which slides a slide show plays, and in what order: every slide but the
// hidden ones (as powerpoint skips them), except that a show started from
// a hidden slide begins with it.

export function playList(slides: { id: string; hidden: boolean }[], startId: string | null): string[] {
  return slides.filter((slide) => !slide.hidden || slide.id === startId).map((slide) => slide.id);
}

/** where the show is after a step; `null` past the last slide (the end screen). */
export function step(position: number, length: number, by: 1 | -1): number | null {
  const next = position + by;
  if (next < 0) return 0;
  if (next >= length) return null;
  return next;
}

/** "12:05", the presenter's clock since the show began. */
export function elapsed(milliseconds: number): string {
  const seconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const two = (value: number) => String(value).padStart(2, "0");
  return hours > 0 ? `${hours}:${two(minutes % 60)}:${two(seconds % 60)}` : `${two(minutes)}:${two(seconds % 60)}`;
}
