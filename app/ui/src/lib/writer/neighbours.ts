// what the writer takes from the rest of the app: a search hit that opens
// the document at its paragraph with the words in Find (lib/library
// arrival.ts), and a file moved while open (the "document-moved" event
// organising sends), after which Save writes to the new place.

import { listen } from "@tauri-apps/api/event";
import { findTextFor, takeArrival } from "../library/arrival";
import { blockPos } from "./textBlocks";
import type { WriterSession } from "./writerSession.svelte";

/** the event organising sends when an open document's file moved (organise_commands.rs) */
const DOCUMENT_MOVED = "document-moved";

/** same file, whatever the slashes and letter case (windows paths) */
const samePath = (a: string, b: string) => a.replace(/\//g, "\\").toLowerCase() === b.replace(/\//g, "\\").toLowerCase();

/** follows the document's file when it is moved; returns the function that stops listening. */
export function followMoves(session: WriterSession): () => void {
  const stop = listen<{ from: string; to: string }>(DOCUMENT_MOVED, (event) => {
    if (samePath(event.payload.from, session.path)) session.followMove(event.payload.to);
  });
  return () => {
    stop.then((unlisten) => unlisten());
  };
}

/** a search result asked to open here: go to its paragraph and put its words in Find. */
export function arriveAt(session: WriterSession, openFind: (query: string) => void) {
  const arrival = takeArrival(session.path);
  if (!arrival) return;
  const doc = session.view?.state.doc ?? session.editor.doc;
  if (arrival.block !== null && arrival.block !== undefined && arrival.block < doc.childCount) {
    session.goTo(blockPos(doc, arrival.block) + 1);
  }
  const query = findTextFor(arrival.query);
  if (query) openFind(query);
}
