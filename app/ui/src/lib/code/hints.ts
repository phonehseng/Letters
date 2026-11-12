// plain-English hints for the mistakes beginners make most, shown under
// the compiler's own message. the raw message always stays: the hint
// explains it, it never replaces it. matched on the wording gcc, clang and
// the in-app html check use.

interface HintRule {
  pattern: RegExp;
  hint: (match: RegExpMatchArray) => string;
}

const C_HINTS: HintRule[] = [
  {
    pattern: /expected ['‘]?;['’]? before|expected ['‘]?,['’]? or ['‘]?;['’]?|expected ';' after/,
    hint: () => "A semicolon is probably missing at the end of the line before this one. In C every statement ends with ;.",
  },
  {
    pattern: /['‘](\w+)['’] undeclared|use of undeclared identifier ['‘](\w+)['’]/,
    hint: (match) => `“${match[1] ?? match[2]}” is used before it is declared. Check the spelling, or declare it first (for example int ${match[1] ?? match[2]};).`,
  },
  {
    pattern: /implicit declaration of function ['‘](\w+)['’]|call to undeclared function ['‘](\w+)['’]/,
    hint: (match) => `C doesn’t know “${match[1] ?? match[2]}” yet. Add the #include for its header at the top, or check the spelling.`,
  },
  {
    pattern: /expected ['‘]?\}['’]? at end of input|expected ['‘]\}['’]/,
    hint: () => "A closing brace } is missing. Every { needs a matching }.",
  },
  {
    pattern: /expected declaration or statement at end of input/,
    hint: () => "The file ends while something is still open: usually a missing }.",
  },
  {
    pattern: /fatal error: ([^:]+): No such file or directory|['‘]([^'’]+)['’] file not found/,
    hint: (match) => `The file “${match[1] ?? match[2]}” wasn’t found. Check its name, and that it is in the same folder or installed.`,
  },
  {
    pattern: /format ['‘]%(\w+)['’] expects/,
    hint: () => "The % code in printf or scanf doesn’t match the type of the value given for it (for example %d is for int, %f for double).",
  },
  {
    pattern: /unused variable ['‘](\w+)['’]/,
    hint: (match) => `“${match[1]}” is declared but never used. It is harmless, but often a sign of a typo.`,
  },
  {
    pattern: /missing terminating ["'] character/,
    hint: () => "A string or character is missing its closing quote.",
  },
];

const HTML_HINTS: HintRule[] = [
  {
    pattern: /<(\w+)> is never closed/,
    hint: (match) => `Add </${match[1]}> where the ${match[1]} should end. Most HTML tags come in pairs.`,
  },
  {
    pattern: /<\/(\w+)> closes <(\w+)>/,
    hint: (match) => `Tags must close in the reverse order they opened: close <${match[2]}> before </${match[1]}>.`,
  },
  {
    pattern: /has no alt text/,
    hint: () => "Screen readers read the alt text aloud instead of the picture. Describe what the picture shows, or use alt=\"\" for decoration.",
  },
  {
    pattern: /The id "([^"]+)" is used/,
    hint: (match) => `An id must be unique on the page. Rename one of the “${match[1]}” ids, or use a class for both.`,
  },
];

const PYTHON_HINTS: HintRule[] = [
  {
    pattern: /expected ['‘]:['’]/,
    hint: () => "Lines that start a block (if, for, while, def, class) end with a colon.",
  },
  {
    pattern: /unexpected indent|unindent does not match|expected an indented block/,
    hint: () => "Python uses indentation to group lines. Indent each line of a block by the same amount (4 spaces is usual).",
  },
];

function rulesFor(language: string): HintRule[] {
  if (language === "c" || language === "cpp") return C_HINTS;
  if (language === "html") return HTML_HINTS;
  if (language === "python") return PYTHON_HINTS;
  return [];
}

/** the hint for a message in a language, or undefined when there isn't one. */
export function hintFor(language: string, message: string): string | undefined {
  for (const rule of rulesFor(language)) {
    const match = message.match(rule.pattern);
    if (match) return rule.hint(match);
  }
  return undefined;
}
