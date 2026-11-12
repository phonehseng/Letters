// the document language picker: languages by the names people know,
// each with the code screen readers need (BCP 47, as PDF asks for).

export interface LanguageChoice {
  code: string;
  name: string;
}

export const LANGUAGES: LanguageChoice[] = [
  { code: "en-US", name: "English (United States)" },
  { code: "en-GB", name: "English (United Kingdom)" },
  { code: "en-AU", name: "English (Australia)" },
  { code: "en-CA", name: "English (Canada)" },
  { code: "en", name: "English" },
  { code: "ar", name: "Arabic" },
  { code: "zh-Hans", name: "Chinese (Simplified)" },
  { code: "zh-Hant", name: "Chinese (Traditional)" },
  { code: "cs", name: "Czech" },
  { code: "da", name: "Danish" },
  { code: "nl", name: "Dutch" },
  { code: "fi", name: "Finnish" },
  { code: "fr", name: "French" },
  { code: "fr-CA", name: "French (Canada)" },
  { code: "de", name: "German" },
  { code: "el", name: "Greek" },
  { code: "he", name: "Hebrew" },
  { code: "hi", name: "Hindi" },
  { code: "hu", name: "Hungarian" },
  { code: "id", name: "Indonesian" },
  { code: "ga", name: "Irish" },
  { code: "it", name: "Italian" },
  { code: "ja", name: "Japanese" },
  { code: "ko", name: "Korean" },
  { code: "la", name: "Latin" },
  { code: "ms", name: "Malay" },
  { code: "nb", name: "Norwegian" },
  { code: "fa", name: "Persian" },
  { code: "pl", name: "Polish" },
  { code: "pt-BR", name: "Portuguese (Brazil)" },
  { code: "pt-PT", name: "Portuguese (Portugal)" },
  { code: "ro", name: "Romanian" },
  { code: "ru", name: "Russian" },
  { code: "es", name: "Spanish" },
  { code: "es-MX", name: "Spanish (Mexico)" },
  { code: "sv", name: "Swedish" },
  { code: "th", name: "Thai" },
  { code: "tr", name: "Turkish" },
  { code: "uk", name: "Ukrainian" },
  { code: "vi", name: "Vietnamese" },
  { code: "cy", name: "Welsh" },
];

/** the shape of a language code: a language, then optional parts such as a script or region. */
export function looksLikeLanguageCode(code: string): boolean {
  return /^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$/.test(code.trim());
}

/**
 * the choice to start the picker at: the engine's suggestion, else the
 * computer's own language, else plain English. a code not in the list is
 * matched by its language alone ("fr-BE" picks French).
 */
export function startingLanguage(suggested: string | null, system: string | undefined): string {
  for (const wanted of [suggested, system]) {
    if (!wanted) continue;
    const exact = LANGUAGES.find((choice) => choice.code.toLowerCase() === wanted.toLowerCase());
    if (exact) return exact.code;
    const base = wanted.split("-")[0].toLowerCase();
    const sameLanguage = LANGUAGES.find((choice) => choice.code.toLowerCase() === base);
    if (sameLanguage) return sameLanguage.code;
  }
  return "en";
}

/** the name of a code for the "will set" sentence, or the code itself. */
export function languageName(code: string): string {
  return LANGUAGES.find((choice) => choice.code === code)?.name ?? code;
}
