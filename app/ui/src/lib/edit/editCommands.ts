// the Edit tab's commands, for the menu bar (the Content menu) and the
// keyboard. commands.ts adds them to the app's one list, so the menu and
// the keys go through the same dispatcher as everything else.

export const EDIT_COMMANDS = ["edit-text", "add-page-text", "add-page-image"] as const;

export type EditCommand = (typeof EDIT_COMMANDS)[number];

/**
 * ctrl and alt with W (control and command on a mac), read from the
 * physical key like the other such keys: W for the words on the page.
 * pressed again it puts Edit Text away.
 */
export const EDIT_KEYS: Record<string, EditCommand> = {
  KeyW: "edit-text",
};
