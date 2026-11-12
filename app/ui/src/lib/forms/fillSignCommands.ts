// the Fill & Sign commands, for the menu bar (the Forms menu) and the
// keyboard. commands.ts adds them to the app's one list, so the menu and
// the keys go through the same dispatcher as everything else.

export const FILL_SIGN_COMMANDS = [
  "fill-form",
  "next-empty-field",
  "toggle-fields",
  "find-fields",
  "add-field",
  "edit-fields",
  "flatten-form",
  "sign-with-picture",
  "sign-with-certificate",
  "show-signatures",
] as const;

export type FillSignCommand = (typeof FILL_SIGN_COMMANDS)[number];

/**
 * the Fill & Sign keys, a letter with ctrl and alt (command and option on
 * a mac), read from `code` like the other such keys. E for "empty":
 * Next Empty Field is how a form is filled from the keyboard.
 */
export const FILL_SIGN_KEYS: Record<string, FillSignCommand> = {
  KeyE: "next-empty-field",
};
