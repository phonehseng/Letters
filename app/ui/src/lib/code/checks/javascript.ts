// the javascript check: acorn's parser (MIT), loaded only when a
// javascript file is checked. it reports the first syntax error with its
// exact place, the same thing node --check would say.

import type { Problem } from "../problems";

export const JAVASCRIPT_SOURCE = "Letters (JavaScript check)";

interface AcornError extends SyntaxError {
  loc?: { line: number; column: number };
  pos?: number;
}

export async function checkJavaScript(text: string): Promise<Problem[]> {
  const acorn = await import("acorn");
  const parse = (sourceType: "module" | "script") =>
    acorn.parse(text, { ecmaVersion: "latest", sourceType, allowHashBang: true, allowReturnOutsideFunction: sourceType === "script", locations: true });
  try {
    parse("module");
    return [];
  } catch (moduleError) {
    // an old-style script (with "with" or octal numbers) is fine as a script
    try {
      parse("script");
      return [];
    } catch {
      return [problemFrom(moduleError as AcornError)];
    }
  }
}

function problemFrom(error: AcornError): Problem {
  const line = error.loc?.line ?? 1;
  // acorn counts columns from 0; people and the editor from 1
  const column = (error.loc?.column ?? 0) + 1;
  const message = error.message.replace(/\s*\(\d+:\d+\)$/, "");
  return { line, column, endLine: line, endColumn: column + 1, severity: "error", message, source: JAVASCRIPT_SOURCE };
}
