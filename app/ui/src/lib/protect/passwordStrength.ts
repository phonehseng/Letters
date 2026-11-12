// a short hint about how hard a password would be to guess.
//
// the open password is real encryption, so a weak one is the weak point.
// the hint is a rough guide, not a promise: it checks length, the kinds
// of characters, and a few very common passwords and patterns, and it
// says what to do, the way the system's own password fields do.

export type StrengthLevel = "empty" | "weak" | "fair" | "strong";

export interface Strength {
  level: StrengthLevel;
  hint: string;
}

// a few of the most common passwords, lower case
const COMMON = new Set([
  "password",
  "password1",
  "passw0rd",
  "123456",
  "12345678",
  "123456789",
  "1234567890",
  "qwerty",
  "qwertyuiop",
  "letmein",
  "iloveyou",
  "welcome",
  "admin",
  "abc123",
  "monkey",
  "dragon",
  "football",
  "sunshine",
  "princess",
  "111111",
  "000000",
]);

export const MINIMUM_LENGTH = 8;

/** how many of lower case, upper case, digits and other characters it uses. */
export function characterKinds(password: string): number {
  const kinds = [/[a-z]/, /[A-Z]/, /[0-9]/, /[^a-zA-Z0-9]/];
  return kinds.filter((kind) => kind.test(password)).length;
}

/** "aaaaaaaa", "12345678", "abcdefgh": one character, or a straight run. */
export function isPattern(password: string): boolean {
  if (password.length < 3) return false;
  if (new Set(password).size === 1) return true;
  const codes = [...password].map((ch) => ch.codePointAt(0) ?? 0);
  const step = codes[1] - codes[0];
  return Math.abs(step) === 1 && codes.every((code, index) => index === 0 || code - codes[index - 1] === step);
}

export function passwordStrength(password: string): Strength {
  if (password.length === 0) return { level: "empty", hint: "" };
  if (COMMON.has(password.toLowerCase()) || isPattern(password)) {
    return { level: "weak", hint: "This password is easy to guess. Choose another one." };
  }
  if (password.length < MINIMUM_LENGTH) {
    return { level: "weak", hint: `Too short. Use at least ${MINIMUM_LENGTH} characters.` };
  }
  const kinds = characterKinds(password);
  if (password.length >= 14 || (password.length >= 10 && kinds >= 3)) {
    return { level: "strong", hint: "Strong. Keep it somewhere safe: nobody can open the file without it." };
  }
  return { level: "fair", hint: "Fair. A few more words or characters make it much harder to guess." };
}

/** what to say under the second password field. */
export function confirmHint(password: string, confirm: string): string {
  if (confirm.length === 0) return "";
  return password === confirm ? "The passwords match." : "The passwords don’t match.";
}

/** whether the open password can be set: typed twice the same, and not empty. */
export function canUsePassword(password: string, confirm: string): boolean {
  return password.length > 0 && password === confirm;
}
