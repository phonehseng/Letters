// choosing the language a scan is read in.
//
// the reader matches shapes against the letters and words of one
// language, so the wrong language gets letters wrong (German read as
// Spanish turned "Straße" into "Strafie"). two languages are allowed for a
// document that really mixes them, but each one makes reading slower and
// the mix confuses look-alike letters, so no more than two.

import type { OcrLanguage } from "./ocrApi";

export const MAX_LANGUAGES = 2;

/** the computer's language as tesseract's code, for the starting choice. */
const FROM_SYSTEM: Record<string, string> = {
  en: "eng",
  es: "spa",
  fr: "fra",
  de: "deu",
  it: "ita",
  pt: "por",
  nl: "nld",
  sv: "swe",
  da: "dan",
  nb: "nor",
  fi: "fin",
  pl: "pol",
  cs: "ces",
  tr: "tur",
  ru: "rus",
  uk: "ukr",
  el: "ell",
};

/** the starting language: the computer's, when it is installed, else English, else the first. */
export function defaultLanguage(installed: OcrLanguage[], systemLanguage: string): string | null {
  const wanted = FROM_SYSTEM[systemLanguage.toLowerCase().split("-")[0]];
  const codes = installed.map((language) => language.code);
  if (wanted && codes.includes(wanted)) return wanted;
  if (codes.includes("eng")) return "eng";
  return codes[0] ?? null;
}

/** ticks or unticks a language; a third is refused (the list comes back unchanged). */
export function toggleLanguage(chosen: string[], code: string): string[] {
  if (chosen.includes(code)) return chosen.filter((each) => each !== code);
  if (chosen.length >= MAX_LANGUAGES) return chosen;
  return [...chosen, code];
}

/** the line under the language list. */
export function languageAdvice(chosen: string[]): string {
  if (chosen.length === 0) return "Choose the language the document is written in.";
  if (chosen.length >= MAX_LANGUAGES) {
    return "Reading in two languages is slower, and letters that look alike in them get mixed up more often. Keep two only if the pages really mix them.";
  }
  return "Choose the language the document is written in. The wrong one gets letters wrong, especially accents.";
}

/** "English", "English and French". */
export function languageNames(chosen: string[], installed: OcrLanguage[]): string {
  const names = chosen.map((code) => installed.find((language) => language.code === code)?.name ?? code);
  return names.length === 2 ? `${names[0]} and ${names[1]}` : (names[0] ?? "");
}
