// how the window keeps a note still being typed before it saves or leaves.
//
// a note's words live in its pop-up until the note is added, so saving or
// closing the window (or going back to Recents) would miss them. the window
// (App.svelte) calls this first; while a pdf is shown, its annotate session
// puts its answer here (see Viewer.svelte), as the Protect tab does with
// marks not yet applied (protect/protectGuard.ts).

export interface AnnotateGuard {
  /** adds the note being typed, if there is one; resolves once it is in the document */
  keepDrafts: (() => Promise<void>) | null;
}

export const annotateGuard: AnnotateGuard = { keepDrafts: null };

/** keeps the note being typed in the shown document, if any. */
export async function keepAnnotateDrafts(guard: AnnotateGuard = annotateGuard): Promise<void> {
  if (guard.keepDrafts) await guard.keepDrafts();
}
