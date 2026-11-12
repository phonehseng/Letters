// the permissions of "Restrict What Others Can Do": the rows of the
// form, turning them into the engine's options, and saying what a set
// of restrictions means in one sentence.
//
// restrictions are only a request. a file with restrictions but no open
// password opens for anyone, and some apps ignore the request, so every
// sentence here says "asks" rather than "stops".

import type { PermissionOptions, PrintPermission } from "./protectApi";

export type PermissionKey = Exclude<keyof PermissionOptions, "printing">;

/** the yes-or-no rows, in the order they are shown. */
export const PERMISSION_ROWS: { key: PermissionKey; label: string }[] = [
  { key: "copying", label: "Copying text and pictures" },
  { key: "editing", label: "Changing the content" },
  { key: "commenting", label: "Adding comments" },
  { key: "fillingForms", label: "Filling in forms" },
  { key: "assembling", label: "Adding, removing and turning pages" },
];

export const PRINT_CHOICES: { value: PrintPermission; label: string }[] = [
  { value: "none", label: "Not Allowed" },
  { value: "lowQuality", label: "Low Quality" },
  { value: "full", label: "Allowed" },
];

export const ALL_ALLOWED: PermissionOptions = {
  printing: "full",
  copying: true,
  editing: true,
  commenting: true,
  fillingForms: true,
  assembling: true,
};

export function isRestricted(options: PermissionOptions): boolean {
  return (Object.keys(ALL_ALLOWED) as (keyof PermissionOptions)[]).some((key) => options[key] !== ALL_ALLOWED[key]);
}

/** the things that are not allowed, as words for a sentence. */
export function restrictedParts(options: PermissionOptions): string[] {
  const parts: string[] = [];
  if (options.printing === "none") parts.push("printing");
  if (options.printing === "lowQuality") parts.push("printing at full quality");
  const words: Record<PermissionKey, string> = {
    copying: "copying",
    editing: "changing the content",
    commenting: "adding comments",
    fillingForms: "filling in forms",
    assembling: "changing pages",
  };
  for (const row of PERMISSION_ROWS) if (!options[row.key]) parts.push(words[row.key]);
  return parts;
}

/** "a, b and c" */
export function joinWords(words: string[]): string {
  if (words.length <= 1) return words.join("");
  return `${words.slice(0, -1).join(", ")} and ${words[words.length - 1]}`;
}

/** one sentence for a set of restrictions: "Asks other apps not to allow printing and copying." */
export function restrictionSentence(options: PermissionOptions): string {
  const parts = restrictedParts(options);
  if (parts.length === 0) return "Nothing is restricted.";
  return `Asks other apps not to allow ${joinWords(parts)}.`;
}
