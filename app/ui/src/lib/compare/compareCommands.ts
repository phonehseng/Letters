// the Compare commands, by name. they join the app's one command list in
// ../commands.ts, so the macOS menu (app/shell/src/menu.rs) and the keys
// reach them through the same dispatcher.
//
// "compare-with-version" works on the open document; "compare-two-pdfs"
// from Recents. next and previous change also answer Alt with the arrows
// (the keys that move pages in the viewer), see CompareScreen.svelte.

export const COMPARE_COMMANDS = ["compare-with-version", "compare-two-pdfs", "next-change", "previous-change"] as const;

export type CompareCommand = (typeof COMPARE_COMMANDS)[number];
