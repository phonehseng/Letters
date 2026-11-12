// opening Search All Documents with words already typed: Recents' search
// field hands its text over when the scope is switched to Everywhere.
// commands carry no arguments, so the text waits here until the screen
// takes it.

import { appCommands } from "../commands";

let pendingQuery = "";

/** opens the search screen with `query` in its field. */
export function openLibrarySearch(query = "") {
  pendingQuery = query;
  appCommands.dispatch("search-all-documents", "pointer");
}

/** the words handed over, once. */
export function takePendingQuery(): string {
  const query = pendingQuery;
  pendingQuery = "";
  return query;
}
