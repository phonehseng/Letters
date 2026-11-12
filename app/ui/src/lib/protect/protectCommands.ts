// the Protect commands, by name, and their keys. they join the app's one
// command list in ../commands.ts, so the macOS Protect menu
// (app/shell/src/protect_menu.rs) and the keys reach them through the
// same dispatcher.
//
// marking has keys because redaction must work without a pointer: ctrl
// and alt with R (mark text) or B (mark area, a box) on windows, control
// and command on a mac, like the Annotate keys. the rest open the tab.

export const PROTECT_COMMANDS = [
  "mark-redaction",
  "mark-redaction-area",
  "find-and-mark",
  "review-redactions",
  "apply-redactions",
  "remove-hidden-information",
  "password-settings",
] as const;

export type ProtectCommand = (typeof PROTECT_COMMANDS)[number];

/** read from the physical key, like the Annotate keys in ../commands.ts. */
export const PROTECT_KEYS: Record<string, ProtectCommand> = {
  KeyR: "mark-redaction",
  KeyB: "mark-redaction-area",
};
