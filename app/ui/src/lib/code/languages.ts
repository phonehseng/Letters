// the languages the code editor knows: their names, file extensions, how
// comments are written, and a guess from the text when the extension
// doesn't say. pure, so it is tested without an editor (languages.test.ts).

export type LanguageId =
  | "html"
  | "css"
  | "javascript"
  | "typescript"
  | "json"
  | "markdown"
  | "xml"
  | "c"
  | "cpp"
  | "python"
  | "java"
  | "rust"
  | "sql"
  | "shell"
  | "yaml"
  | "toml"
  | "text";

export interface LanguageInfo {
  id: LanguageId;
  /** the name people know, shown in the status bar */
  label: string;
  extensions: string[];
}

export const LANGUAGES: LanguageInfo[] = [
  { id: "html", label: "HTML", extensions: ["html", "htm", "xhtml"] },
  { id: "css", label: "CSS", extensions: ["css"] },
  { id: "javascript", label: "JavaScript", extensions: ["js", "mjs", "cjs", "jsx"] },
  { id: "typescript", label: "TypeScript", extensions: ["ts", "mts", "cts", "tsx"] },
  { id: "json", label: "JSON", extensions: ["json", "jsonc", "webmanifest"] },
  { id: "markdown", label: "Markdown", extensions: ["md", "markdown", "mdown"] },
  { id: "xml", label: "XML", extensions: ["xml", "svg", "xsd", "xsl", "plist", "csproj"] },
  { id: "c", label: "C", extensions: ["c", "h"] },
  { id: "cpp", label: "C++", extensions: ["cpp", "cc", "cxx", "hpp", "hh", "hxx", "ino"] },
  { id: "python", label: "Python", extensions: ["py", "pyw", "pyi"] },
  { id: "java", label: "Java", extensions: ["java"] },
  { id: "rust", label: "Rust", extensions: ["rs"] },
  { id: "sql", label: "SQL", extensions: ["sql"] },
  { id: "shell", label: "Shell Script", extensions: ["sh", "bash", "zsh"] },
  { id: "yaml", label: "YAML", extensions: ["yaml", "yml"] },
  { id: "toml", label: "TOML", extensions: ["toml"] },
  { id: "text", label: "Plain Text", extensions: ["txt", "text", "log", "csv", "tsv", "ini", "cfg", "conf"] },
];

/** extensions the window sends to the code editor rather than refusing. */
export const CODE_EXTENSIONS: string[] = LANGUAGES.flatMap((language) => language.extensions);

export function languageInfo(id: LanguageId): LanguageInfo {
  return LANGUAGES.find((language) => language.id === id) ?? LANGUAGES[LANGUAGES.length - 1];
}

function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/** the language a file name says, or null when the extension is unknown. */
export function languageForPath(path: string): LanguageId | null {
  const extension = extensionOf(path);
  const name = (path.split(/[\\/]/).pop() ?? "").toLowerCase();
  // files known by their whole name, with no telling extension
  if (name === "makefile" || name === "dockerfile") return "shell";
  if (name === "cargo.lock") return "toml";
  const found = LANGUAGES.find((language) => language.extensions.includes(extension));
  return found?.id ?? null;
}

/** "C file", "Plain Text file" for Recents, or null for a file that isn't code. */
export function codeFileKind(path: string): string | null {
  const language = languageForPath(path);
  return language ? `${languageInfo(language).label} file` : null;
}

/** true when the window should open this path in the code editor. */
export function isCodePath(path: string): boolean {
  return languageForPath(path) !== null;
}

// the first line that settles the question on its own
const FIRST_LINE_RULES: [RegExp, LanguageId][] = [
  [/^#!.*\bpython/, "python"],
  [/^#!.*\b(ba|z)?sh\b/, "shell"],
  [/^#!.*\bnode\b/, "javascript"],
  [/^<\?xml/, "xml"],
  [/^<!doctype html/i, "html"],
];

// words that are common in one language and rare elsewhere, with weights
const KEYWORD_RULES: [RegExp, LanguageId, number][] = [
  [/^\s*#include\s*[<"]/m, "c", 3],
  [/\bstd::|\bcout\s*<<|^\s*#include\s*<(iostream|vector|string)>/m, "cpp", 4],
  [/\bint\s+main\s*\(/, "c", 2],
  [/\bprintf\s*\(|\bscanf\s*\(|\bmalloc\s*\(/, "c", 2],
  [/^\s*def\s+\w+\s*\(.*\)\s*:/m, "python", 3],
  [/^\s*(import \w+|from \w+ import)/m, "python", 2],
  [/\bprint\s*\(|\bself\b|\belif\b/, "python", 1],
  [/\bpublic\s+(static\s+)?(class|void)\b|System\.out\.println/, "java", 4],
  [/\bfn\s+\w+\s*\(|\blet\s+mut\b|println!\s*\(/, "rust", 4],
  [/\b(function|const|let)\s+\w+|console\.log\s*\(|=>/, "javascript", 1],
  [/\bdocument\.|addEventListener\s*\(/, "javascript", 2],
  [/^\s*<(html|head|body|div|p|h1|script|!--)\b/im, "html", 3],
  [/^\s*[.#]?[\w-]+\s*\{[^}]*:[^}]*;/m, "css", 2],
  [/\b(SELECT|INSERT INTO|CREATE TABLE|UPDATE)\b[\s\S]*\b(FROM|VALUES|SET|\()/i, "sql", 3],
  [/^\s*[\w-]+:\s+\S+$/m, "yaml", 1],
  [/^\s*\[[\w.-]+\]\s*$/m, "toml", 1],
  [/^#{1,6}\s+\S/m, "markdown", 2],
  [/^\s*(echo|export|if \[)\b/m, "shell", 1],
];

/**
 * a guess from the text alone, for code found in a document or a file
 * with no extension. returns "text" when nothing stands out.
 */
export function guessLanguage(text: string): LanguageId {
  const sample = text.slice(0, 20000);
  const firstLine = sample.split("\n", 1)[0].trim();
  for (const [pattern, language] of FIRST_LINE_RULES) {
    if (pattern.test(firstLine)) return language;
  }
  if (looksLikeJson(sample)) return "json";
  const scores = new Map<LanguageId, number>();
  for (const [pattern, language, weight] of KEYWORD_RULES) {
    if (pattern.test(sample)) scores.set(language, (scores.get(language) ?? 0) + weight);
  }
  // c++ code also matches the c rules, so c++ wins when its own words appear
  if ((scores.get("cpp") ?? 0) > 0) scores.set("cpp", (scores.get("cpp") ?? 0) + (scores.get("c") ?? 0));
  let best: LanguageId = "text";
  let bestScore = 1;
  for (const [language, score] of scores) {
    if (score > bestScore) {
      best = language;
      bestScore = score;
    }
  }
  return best;
}

function looksLikeJson(sample: string): boolean {
  const trimmed = sample.trim();
  if (!/^[[{]/.test(trimmed)) return false;
  try {
    JSON.parse(trimmed);
    return true;
  } catch {
    // an object that fails to parse may still be json with a mistake in it
    return /^\{\s*"[^"]+"\s*:/.test(trimmed);
  }
}
