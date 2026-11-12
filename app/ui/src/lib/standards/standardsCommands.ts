// the Prepare & Check command, for the document menu and the menu bar.
// commands.ts adds it to the app's one list, so every way in goes through
// the same dispatcher.

export const STANDARDS_COMMANDS = ["prepare-and-check"] as const;

export type StandardsCommand = (typeof STANDARDS_COMMANDS)[number];
