// files dragged in from the system. the window as a whole opens dropped
// pdfs, but a part of it (the thumbnails) can claim a drop to do something
// else with it, such as inserting the pages where they were dropped.
//
// the system reports the pointer in physical pixels; the page works in
// css pixels, so positions are divided by the screen's pixel ratio.

export interface FileDropTarget {
  /** files are being dragged over the window. returns true when this target is under the pointer */
  over: (x: number, y: number) => boolean;
  /** files were dropped. returns true when this target took them */
  drop: (paths: string[], point: { x: number; y: number } | null) => boolean;
  /** the drag left the window or was cancelled */
  leave: () => void;
}

let current: FileDropTarget | null = null;

/** makes `target` the one that may claim drops. returns a function that takes it back. */
export function setFileDropTarget(target: FileDropTarget): () => void {
  current = target;
  return () => {
    if (current === target) current = null;
  };
}

export function fileDropTarget(): FileDropTarget | null {
  return current;
}

/** a position from the system in css pixels, or null when there is none. */
export function toCssPoint(position: { x: number; y: number } | undefined | null, pixelRatio: number): { x: number; y: number } | null {
  if (!position || !Number.isFinite(position.x) || !Number.isFinite(position.y)) return null;
  const ratio = pixelRatio > 0 ? pixelRatio : 1;
  return { x: position.x / ratio, y: position.y / ratio };
}

/** only pdfs can be inserted; other files are left for the window to deal with. */
export function pdfsAmong(paths: string[]): string[] {
  return paths.filter((path) => /\.pdf$/i.test(path));
}
