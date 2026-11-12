// what happens while someone types: smart quotes, autocorrect and
// capitals at the start of sentences.
//
// this is a line-by-line port of `on_type` in core/office/src/typing.rs,
// with the same test cases (typing.test.ts). it runs in the editor on
// every keystroke: a call into the shell would cross a process boundary
// and wait for the shell's thread pool, a few milliseconds each time,
// which is enough to feel typing lag behind on a slow machine. the pure
// function costs microseconds. keep the two in step: the rust tests are
// the reference.

export interface AutocorrectRule {
  from: string;
  to: string;
}

/** the engine's switches (TypingSettings in typing.rs), all on by default. */
export interface TypingSettings {
  smartQuotes: boolean;
  capitalizeSentences: boolean;
  capitalizeI: boolean;
  fixTwoInitialCapitals: boolean;
  replaceDashes: boolean;
  replaceSymbols: boolean;
  correctSpelling: boolean;
  language: string;
  customRules: AutocorrectRule[];
}

export function defaultTypingSettings(): TypingSettings {
  return {
    smartQuotes: true,
    capitalizeSentences: true,
    capitalizeI: true,
    fixTwoInitialCapitals: true,
    replaceDashes: true,
    replaceSymbols: true,
    correctSpelling: true,
    language: "en-US",
    customRules: [],
  };
}

export interface Replacement {
  /** characters (not utf-16 units) to remove before the cursor */
  deleteBefore: number;
  /** what to insert; it stands in for the typed character too */
  insert: string;
  feature: string;
}

const replacement = (deleteBefore: number, insert: string, feature: string): Replacement => ({ deleteBefore, insert, feature });

/** applies a replacement to plain text, as the rust tests do. */
export function applyReplacement(before: string, change: Replacement): string {
  const chars = Array.from(before);
  return chars.slice(0, Math.max(0, chars.length - change.deleteBefore)).join("") + change.insert;
}

// char classes as rust's char methods define them
const isLowercase = (c: string) => /\p{Lowercase}/u.test(c);
const isUppercase = (c: string) => /\p{Uppercase}/u.test(c);
const isAlphabetic = (c: string) => /\p{Alphabetic}/u.test(c);
const isAlphanumeric = (c: string) => /[\p{Alphabetic}\p{N}]/u.test(c);
const isWhitespace = (c: string) => /\p{White_Space}/u.test(c);
const isAsciiDigit = (c: string) => c >= "0" && c <= "9";

const lastChar = (text: string): string | undefined => Array.from(text).at(-1);
/** like rust's `chars().rev().nth(n)` */
const charFromEnd = (text: string, n: number): string | undefined => Array.from(text).reverse()[n];

/** decides what typing `typed` after `before` should produce, or null. */
export function onType(before: string, typed: string, settings: TypingSettings): Replacement | null {
  if (settings.smartQuotes && (typed === '"' || typed === "'")) return smartQuote(before, typed, settings.language);
  if (settings.replaceSymbols) {
    const found = symbol(before, typed);
    if (found) return found;
  }
  if (settings.replaceDashes) {
    const found = dash(before, typed);
    if (found) return found;
  }
  if (isWordEnd(typed)) {
    const found = finishedWord(before, typed, settings);
    if (found) return found;
  }
  if (settings.capitalizeSentences && isLowercase(typed) && startsSentence(before)) {
    return replacement(0, typed.toUpperCase(), "capitalizeSentences");
  }
  return null;
}

function quoteMarks(language: string): [string, string, string, string] {
  const primary = (language.split(/[-_]/)[0] || "en").toLowerCase();
  if (["de", "cs", "sk", "sl", "lt", "et", "is"].includes(primary)) return ["„", "“", "‚", "‘"];
  if (primary === "fr") return ["« ", " »", "‹ ", " ›"];
  if (["es", "it", "pt", "ca", "el", "nb", "no"].includes(primary)) return ["«", "»", "“", "”"];
  if (["ru", "uk", "be"].includes(primary)) return ["«", "»", "„", "“"];
  if (["pl", "hu", "ro", "hr", "bg"].includes(primary)) return ["„", "”", "‚", "’"];
  if (["sv", "fi"].includes(primary)) return ["”", "”", "’", "’"];
  if (["ja", "zh"].includes(primary)) return ["「", "」", "『", "』"];
  return ["“", "”", "‘", "’"];
}

const APOSTROPHE = "’";
const PRIME = "′";
const DOUBLE_PRIME = "″";

const countOf = (text: string, part: string) => (part ? text.split(part).length - 1 : 0);

function smartQuote(before: string, typed: string, language: string): Replacement {
  const marks = quoteMarks(language);
  const double = typed === '"';
  const previous = lastChar(before);
  // 5'10" is feet and inches: primes, not quotes
  if (previous !== undefined && isAsciiDigit(previous) && isMeasurement(before)) {
    return replacement(0, double ? DOUBLE_PRIME : PRIME, "smartQuotes");
  }
  const [open, close] = double ? [marks[0], marks[1]] : [marks[2], marks[3]];
  // french puts a space before the closing mark, so after a space with a
  // quote still open, the mark closes it
  const closesAfterSpace = previous === " " && close.startsWith(" ") && countOf(before, open) > countOf(before, close);
  const opening =
    !closesAfterSpace &&
    (previous === undefined || isWhitespace(previous) || "([{<—–-/".includes(previous) || isOpeningQuote(previous));
  // inside or after a word: don't, it's, the students'
  if (!double && !opening) return replacement(0, APOSTROPHE, "smartQuotes");
  if (opening) return replacement(0, open, "smartQuotes");
  // french closing marks bring their own no-break space
  if (close.startsWith(" ") && previous === " ") return replacement(1, close, "smartQuotes");
  return replacement(0, close, "smartQuotes");
}

function isOpeningQuote(c: string): boolean {
  return ["“", "‘", "„", "‚", "«", "‹", "「", "『"].includes(c);
}

/** true when the word before the cursor is a measurement such as 5 or 5′10. */
function isMeasurement(before: string): boolean {
  const reversed: string[] = [];
  for (const c of Array.from(before).reverse()) {
    if (isWhitespace(c)) break;
    reversed.push(c);
  }
  const word = reversed.join("");
  // reversed, so the first number is last; a long number is a year
  const pieces = word.split(/[′″]/);
  const firstNumber = pieces[pieces.length - 1] ?? "";
  return (
    word.length > 0 &&
    Array.from(firstNumber).length <= 3 &&
    Array.from(word).every((c) => isAsciiDigit(c) || c === "." || c === PRIME || c === DOUBLE_PRIME)
  );
}

function symbol(before: string, typed: string): Replacement | null {
  const lower = before.toLowerCase();
  let found: [number, string] | null = null;
  if (typed === ")" && lower.endsWith("(c")) found = [2, "©"];
  else if (typed === ")" && lower.endsWith("(r")) found = [2, "®"];
  else if (typed === ")" && lower.endsWith("(tm")) found = [3, "™"];
  else if (typed === "." && before.endsWith("..")) found = [2, "…"];
  else if (typed === ">" && before.endsWith("-")) found = [1, "→"];
  else if (typed === ">" && before.endsWith("=")) found = [1, "⇒"];
  else if (typed === "-" && before.endsWith("<")) found = [1, "←"];
  return found ? replacement(found[0], found[1], "replaceSymbols") : null;
}

function dash(before: string, typed: string): Replacement | null {
  // "word--" becomes "word—" as soon as the second hyphen is typed
  if (typed === "-" && before.endsWith("-") && !before.endsWith("--")) {
    const ahead = charFromEnd(before, 1);
    if (ahead !== undefined && (isAlphanumeric(ahead) || ahead === " ")) return replacement(1, "—", "replaceDashes");
  }
  // "word - " becomes "word – " when the space after the hyphen is typed
  if (typed === " " && before.endsWith(" -")) {
    const ahead = charFromEnd(before, 2);
    if (ahead !== undefined && isAlphanumeric(ahead)) return replacement(1, "– ", "replaceDashes");
  }
  return null;
}

function isWordEnd(c: string): boolean {
  return isWhitespace(c) || ".,;:!?)]}\"'”’".includes(c);
}

/** the letters (and apostrophes) just before the cursor. */
function lastWord(before: string): string {
  const chars = Array.from(before);
  let start = chars.length;
  while (start > 0 && (isAlphanumeric(chars[start - 1]) || chars[start - 1] === "'" || chars[start - 1] === APOSTROPHE)) start -= 1;
  return chars.slice(start).join("");
}

function finishedWord(before: string, typed: string, settings: TypingSettings): Replacement | null {
  const word = lastWord(before);
  if (!word) return null;
  const length = Array.from(word).length;
  const finish = (fixed: string, feature: string) => replacement(length, fixed + typed, feature);
  for (const rule of settings.customRules) if (rule.from === word) return finish(rule.to, "customRules");
  const english = settings.language.toLowerCase().startsWith("en");
  if (settings.capitalizeI && english) {
    const fixed = capitalI(word);
    if (fixed) return finish(fixed, "capitalizeI");
  }
  if (settings.fixTwoInitialCapitals) {
    const fixed = twoInitialCapitals(word);
    if (fixed) return finish(fixed, "fixTwoInitialCapitals");
  }
  if (settings.correctSpelling && english) {
    const fixed = commonTypo(word);
    if (fixed) return finish(fixed, "correctSpelling");
  }
  return null;
}

function capitalI(word: string): string | null {
  const normal = word.replaceAll("'", APOSTROPHE);
  if (normal === "i") return "I";
  if (["i’m", "i’ll", "i’d", "i’ve"].includes(normal)) return `I${normal.slice(1)}`;
  return null;
}

function twoInitialCapitals(word: string): string | null {
  const chars = Array.from(word);
  if (chars.length < 3 || !chars.every(isAlphabetic)) return null;
  const restLower = chars.slice(2).every(isLowercase);
  if (isUppercase(chars[0]) && isUppercase(chars[1]) && restLower) return chars[0] + chars[1].toLowerCase() + chars.slice(2).join("");
  return null;
}

/** slips that are never real words (the same list as typing.rs). */
const TYPOS: [string, string][] = [
  ["teh", "the"], ["adn", "and"], ["taht", "that"], ["hte", "the"], ["recieve", "receive"], ["recieved", "received"],
  ["beleive", "believe"], ["seperate", "separate"], ["definately", "definitely"], ["occured", "occurred"],
  ["occuring", "occurring"], ["untill", "until"], ["wich", "which"], ["thier", "their"], ["accomodate", "accommodate"],
  ["acheive", "achieve"], ["arguement", "argument"], ["enviroment", "environment"], ["goverment", "government"],
  ["independant", "independent"], ["neccessary", "necessary"], ["noticable", "noticeable"], ["persue", "pursue"],
  ["publically", "publicly"], ["recomend", "recommend"], ["refered", "referred"], ["succesful", "successful"],
  ["tommorow", "tomorrow"], ["truely", "truly"], ["wierd", "weird"], ["alot", "a lot"], ["dont", "don’t"],
  ["doesnt", "doesn’t"], ["didnt", "didn’t"], ["isnt", "isn’t"], ["wasnt", "wasn’t"],
  ["couldnt", "couldn’t"], ["shouldnt", "shouldn’t"], ["wouldnt", "wouldn’t"],
];

function commonTypo(word: string): string | null {
  const found = TYPOS.find(([typo]) => typo === word.toLowerCase());
  if (!found) return null;
  const fixed = found[1];
  const first = Array.from(word)[0];
  if (!isUppercase(first)) return fixed;
  const [head, ...rest] = Array.from(fixed);
  return head.toUpperCase() + rest.join("");
}

const ABBREVIATIONS = [
  "mr", "mrs", "ms", "dr", "prof", "st", "jr", "sr", "vs", "etc", "e.g", "i.e", "cf", "al", "p",
  "pp", "no", "fig", "vol", "ch", "ed", "eds", "approx", "dept", "est", "inc", "ltd", "co", "a.m", "p.m",
];

/** true when a letter typed after `before` would begin a sentence. */
function startsSentence(before: string): boolean {
  const trimmed = before.replace(/\p{White_Space}+$/u, "");
  if (!trimmed) return true;
  // no space typed yet: this letter continues the current word
  if (trimmed.length === before.length) return false;
  // look past closing quotes and brackets: He said "Stop." Then...
  const core = trimmed.replace(/["'”’)\]»]+$/u, "");
  const last = lastChar(core);
  if (last === undefined) return false;
  if (last === "!" || last === "?") return true;
  if (last !== "." || core.endsWith("..")) return false;
  const pieces = core.slice(0, -1).split(/[\p{White_Space}("]/u);
  const word = (pieces[pieces.length - 1] ?? "").toLowerCase();
  if (!word || ABBREVIATIONS.includes(word)) return false;
  // "J." is an initial and "3." a number in a list, not a sentence end
  const chars = Array.from(word);
  const singleLetter = chars.length === 1 && chars.every(isAlphabetic);
  const number = chars.every(isAsciiDigit);
  return !(singleLetter || number);
}

// --- the four switches people see

/** the four switches of the status bar and the Typing section. */
export type TypingToggle = "autocorrect" | "capitalize" | "smartQuotes" | "spellcheck";

/** which engine switches each visible switch turns on and off. */
export const TOGGLE_FEATURES: Record<Exclude<TypingToggle, "spellcheck">, (keyof TypingSettings)[]> = {
  autocorrect: ["correctSpelling", "fixTwoInitialCapitals", "replaceDashes", "replaceSymbols"],
  capitalize: ["capitalizeSentences", "capitalizeI"],
  smartQuotes: ["smartQuotes"],
};

/** a visible switch is on when any of its engine switches is. */
export function toggleIsOn(settings: TypingSettings, toggle: Exclude<TypingToggle, "spellcheck">): boolean {
  return TOGGLE_FEATURES[toggle].some((key) => settings[key] === true);
}

export function setToggle(settings: TypingSettings, toggle: Exclude<TypingToggle, "spellcheck">, on: boolean): TypingSettings {
  const next = { ...settings };
  for (const key of TOGGLE_FEATURES[toggle]) (next as unknown as Record<string, unknown>)[key] = on;
  return next;
}

/** which visible switch made a change, so a message can offer to turn it off. */
export function toggleForFeature(feature: string): Exclude<TypingToggle, "spellcheck"> | null {
  for (const [toggle, features] of Object.entries(TOGGLE_FEATURES)) {
    if ((features as string[]).includes(feature)) return toggle as Exclude<TypingToggle, "spellcheck">;
  }
  return null;
}
