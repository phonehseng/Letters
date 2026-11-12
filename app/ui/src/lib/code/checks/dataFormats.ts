// yaml and toml checks through their own parsers (yaml: ISC, smol-toml:
// BSD-3-Clause), each loaded only when a file of that kind is checked.

import type { Problem } from "../problems";

export const YAML_SOURCE = "Letters (YAML check)";
export const TOML_SOURCE = "Letters (TOML check)";

export async function checkYaml(text: string): Promise<Problem[]> {
  const { parseAllDocuments } = await import("yaml");
  const problems: Problem[] = [];
  for (const document of parseAllDocuments(text)) {
    for (const error of document.errors) problems.push(yamlProblem(error, "error"));
    for (const warning of document.warnings) problems.push(yamlProblem(warning, "warning"));
  }
  return problems;
}

interface YamlIssue {
  message: string;
  linePos?: [{ line: number; col: number }, { line: number; col: number }?];
}

function yamlProblem(issue: YamlIssue, severity: Problem["severity"]): Problem {
  const start = issue.linePos?.[0] ?? { line: 1, col: 1 };
  const end = issue.linePos?.[1];
  // the library adds the offending line and a caret under it; the editor shows that itself
  const message = issue.message.split("\n")[0].replace(/ at line \d+, column \d+:?$/, "");
  return { line: start.line, column: start.col, endLine: end?.line, endColumn: end?.col, severity, message, source: YAML_SOURCE };
}

interface TomlIssue {
  message: string;
  line?: number;
  column?: number;
}

export async function checkToml(text: string): Promise<Problem[]> {
  const { parse } = await import("smol-toml");
  try {
    parse(text);
    return [];
  } catch (error) {
    const issue = error as TomlIssue;
    const message = issue.message.split("\n")[0].replace(/^Invalid TOML document: /, "");
    return [{ line: issue.line ?? 1, column: issue.column ?? 1, severity: "error", message, source: TOML_SOURCE }];
  }
}
