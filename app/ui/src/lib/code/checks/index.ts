// the checks that run inside Letters, chosen by language, and one honest
// sentence for each saying what it looks at. a compiler, when installed,
// adds its own problems on top (see diagnostics.ts and CodeScreen).

import type { LanguageId } from "../languages";
import type { Problem } from "../problems";
import { checkBalance } from "./balance";
import { checkCss } from "./css";
import { checkToml, checkYaml } from "./dataFormats";
import { checkHtml } from "./html";
import { checkJavaScript } from "./javascript";
import { checkJson } from "./json";
import { checkMarkdown } from "./markdown";
import { checkXml } from "./xml";

/** problems found by Letters itself, without any program on the computer. */
export async function checkInApp(language: LanguageId, text: string): Promise<Problem[]> {
  switch (language) {
    case "json":
      return checkJson(text);
    case "html":
      return checkHtml(text);
    case "xml":
      return checkXml(text);
    case "css":
      return checkCss(text);
    case "javascript":
      return checkJavaScript(text);
    case "yaml":
      return checkYaml(text);
    case "toml":
      return checkToml(text);
    case "markdown":
      return checkMarkdown(text);
    case "text":
      return [];
    default:
      return checkBalance(text, language);
  }
}

/** what the in-app check covers, in a sentence for the status bar's tooltip and Explain. */
export function inAppCoverage(language: LanguageId): string {
  switch (language) {
    case "json":
      return "Letters checks that this is valid JSON and shows the first mistake.";
    case "html":
      return "Letters checks tags that aren’t closed or close in the wrong order, repeated ids, pictures without alt text and tags that can’t go inside each other. It doesn’t check every HTML rule.";
    case "xml":
      return "Letters checks that the XML is well formed: every tag closed in order, one root, attributes written once.";
    case "css":
      return "Letters checks braces, colons, missing semicolons and property names the browser doesn’t know.";
    case "javascript":
      return "Letters reads the file with a JavaScript parser and shows the first syntax mistake. It doesn’t check types or run the code.";
    case "yaml":
    case "toml":
      return "Letters reads the file with a parser for this format and shows its mistakes.";
    case "markdown":
      return "Letters checks heading levels, links without an address, links to headings that don’t exist and code blocks left open.";
    case "text":
      return "Plain text has nothing to check.";
    default:
      return "Basic check only: Letters checks that brackets pair up and strings are closed, and reports what the highlighter couldn’t read. It doesn’t know the language’s rules.";
  }
}
