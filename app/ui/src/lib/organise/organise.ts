// the rules of organising documents, with no dom: choosing several
// documents with clicks and keys, the folder picker's list, and the
// sentences (the undo toast, the name clash question, the recycle bin
// question) in Apple's wording.

/** the documents chosen in a list, and the one the next Shift-click counts from. */
export interface Chosen {
  paths: string[];
  anchor: string | null;
}

export const NOTHING_CHOSEN: Chosen = { paths: [], anchor: null };

/**
 * a click on `path` in `list`: alone it chooses only that one, with Ctrl
 * (Command) it is added or taken away, with Shift the run from the anchor
 * is chosen. returns null for a plain click, which opens the document.
 */
export function clickChoose(chosen: Chosen, list: string[], path: string, keys: { toggle: boolean; range: boolean }): Chosen | null {
  if (keys.range && chosen.anchor && list.includes(chosen.anchor)) {
    const [a, b] = [list.indexOf(chosen.anchor), list.indexOf(path)].sort((x, y) => x - y);
    return { paths: list.slice(a, b + 1), anchor: chosen.anchor };
  }
  if (keys.toggle || keys.range) {
    const paths = chosen.paths.includes(path) ? chosen.paths.filter((p) => p !== path) : [...chosen.paths, path];
    return { paths, anchor: path };
  }
  return null;
}

/** keeps only documents still in the list (after a move or a refresh). */
export function keepListed(chosen: Chosen, list: string[]): Chosen {
  const paths = chosen.paths.filter((p) => list.includes(p));
  return { paths, anchor: chosen.anchor && list.includes(chosen.anchor) ? chosen.anchor : null };
}

export function nameOf(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

/** a folder the picker offers, and why ("Pinned", "Search Folder", "Recent"). */
export interface Destination {
  path: string;
  kind: "pinned" | "library" | "recent";
}

/** the picker's list: pinned first, then library folders, then recent places, each once. */
export function destinations(pinned: string[], library: string[], recent: string[], typed: string): Destination[] {
  const seen = new Set<string>();
  const all: Destination[] = [];
  const add = (path: string, kind: Destination["kind"]) => {
    const key = path.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    all.push({ path, kind });
  };
  pinned.forEach((p) => add(p, "pinned"));
  library.forEach((p) => add(p, "library"));
  recent.forEach((p) => add(p, "recent"));
  const filter = typed.trim().toLowerCase();
  return filter ? all.filter((d) => nameOf(d.path).toLowerCase().includes(filter) || d.path.toLowerCase().includes(filter)) : all;
}

/** recently used destinations, newest first, at most eight. */
export function rememberDestination(recent: string[], folder: string): string[] {
  return [folder, ...recent.filter((p) => p.toLowerCase() !== folder.toLowerCase())].slice(0, 8);
}

function documents(count: number): string {
  return count === 1 ? "1 document" : `${count} documents`;
}

/** "Moved 3 documents to Biology 101." */
export function doneSentence(kind: "moved" | "copied" | "renamed", count: number, folder: string): string {
  if (kind === "renamed") return "Renamed.";
  return `${kind === "moved" ? "Moved" : "Copied"} ${documents(count)} to ${nameOf(folder)}.`;
}

/** the name clash question, as the finder asks it. */
export function conflictQuestion(existing: string): string {
  return `An item named “${nameOf(existing)}” already exists in this location. Do you want to replace it with the one you’re moving?`;
}

/** the recycle bin question; macOS calls it the Trash. */
export function recycleQuestion(paths: string[], isMac: boolean): string {
  const bin = isMac ? "Trash" : "Recycle Bin";
  const what = paths.length === 1 ? `“${nameOf(paths[0])}”` : documents(paths.length);
  return `Move ${what} to the ${bin}? You can put ${paths.length === 1 ? "it" : "them"} back from the ${bin}.`;
}
