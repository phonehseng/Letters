// the Accessibility commands, by name, joined into ../commands.ts. the
// macOS menu's Check Accessibility… opens the tab and runs the check.

export const A11Y_COMMANDS = ["check-accessibility"] as const;

export type A11yCommand = (typeof A11Y_COMMANDS)[number];
