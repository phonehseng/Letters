// pinned folders on the recents screen, as the search box filters them.

import type { FolderFile, PinnedFolder } from "./api";
import { matchesSearch } from "./format";

export interface ShownFolder {
  folder: PinnedFolder;
  files: FolderFile[];
}

/**
 * the folders and files to show for what is typed in the search box.
 *
 * with an empty box every folder shows, even one with no pdfs, so its
 * empty state can say so. while searching, a folder with no matching
 * file is left out: it would only be a heading with nothing under it.
 */
export function filterFolders(folders: PinnedFolder[], search: string): ShownFolder[] {
  const searching = search.trim() !== "";
  return folders
    .map((folder) => ({ folder, files: folder.files.filter((file) => matchesSearch(file.name, search)) }))
    .filter((shown) => !searching || shown.files.length > 0);
}
