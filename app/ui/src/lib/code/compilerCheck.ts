// the compiler-backed check of a file: which languages have one, asking
// the shell for it, and the line that says who checked the file, so the
// person knows how far to trust the list ("Checked by gcc 16.2" against
// "Basic check only: no C compiler found on this computer").

import { compilerCheck, type CheckLanguage, type CheckOutcome } from "./codeApi";
import { parseCheckerOutput } from "./diagnostics";
import type { LanguageId } from "./languages";
import type { Problem } from "./problems";

const COMPILER_LANGUAGES: Partial<Record<LanguageId, CheckLanguage>> = {
  c: "c",
  cpp: "cpp",
  python: "python",
  java: "java",
  javascript: "javaScript",
};

export function hasCompilerCheck(language: LanguageId): boolean {
  return language in COMPILER_LANGUAGES;
}

export interface CompilerResult {
  problems: Problem[];
  /** the label of the compiler that answered, or null when none is installed */
  checker: string | null;
  /** the shell's sentence when there was no compiler, or it took too long */
  note: string | null;
}

/** the problems a compiler found in `text`, read from its output. */
export function resultFromOutcome(language: LanguageId, outcome: CheckOutcome): CompilerResult {
  if (!outcome.checker || !outcome.outputFormat) return { problems: [], checker: null, note: outcome.note };
  const problems = parseCheckerOutput({
    format: outcome.outputFormat,
    output: outcome.output,
    fileName: outcome.fileName,
    checker: outcome.checker,
    language,
  });
  return { problems, checker: outcome.checker, note: outcome.note };
}

/** asks the shell's compiler check; null for a language without one. */
export async function runCompilerCheck(language: LanguageId, text: string, path: string | null): Promise<CompilerResult | null> {
  const checkLanguage = COMPILER_LANGUAGES[language];
  if (!checkLanguage) return null;
  const outcome = await compilerCheck(checkLanguage, text, path);
  if (outcome.cancelled) throw new Error("a newer check replaced this one");
  return resultFromOutcome(language, outcome);
}

/** the status bar's line about who checked the file. */
export function checkerLine(language: LanguageId, compiler: CompilerResult | null): string {
  if (compiler?.checker) {
    return compiler.note ?? `Checked by ${compiler.checker}`;
  }
  if (compiler?.note) return compiler.note;
  if (language === "text") return "Plain text isn’t checked";
  if (["json", "html", "xml", "css", "yaml", "toml", "markdown", "javascript"].includes(language)) return "Checked inside Letters";
  return "Basic check only";
}
