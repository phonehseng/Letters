// in-app updates (app/shell/src/updates.rs): a newer version from github
// releases, signed with the release key, installed once every window has
// closed the usual way, so unsaved work is always asked about first.

import { invoke } from "@tauri-apps/api/core";

/** what the update note shows. */
export interface UpdateNote {
  version: string;
  current: string;
  /** what changed, in a few plain lines */
  notes: string;
}

/** the event each window hears when an update is ready: it closes the usual way. */
export const CLOSE_FOR_UPDATE = "letters-close-for-update";

/**
 * asks for a newer version, or null when there is none. `quiet` is the
 * check on launch: at most once a day, only when settings allow it, and
 * silent when offline.
 */
export function checkForUpdate(quiet: boolean): Promise<UpdateNote | null> {
  return invoke("update_check", { quiet });
}

/** downloads and checks the update, then every window is asked to close. */
export function prepareUpdate(): Promise<void> {
  return invoke("update_prepare");
}

/** a window kept its unsaved changes: the update waits. */
export function cancelUpdate(): Promise<void> {
  return invoke("update_cancel");
}

/** the note's heading. */
export function updateHeading(note: UpdateNote): string {
  return `Letters ${note.version} is ready`;
}
