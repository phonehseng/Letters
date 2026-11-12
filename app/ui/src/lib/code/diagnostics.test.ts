// the compiler readers, on output recorded from the real tools on windows
// (gcc 16.2 from winlibs, javac 21, node 26) and from the python check.

import { describe, expect, it } from "vitest";
import { parseCheckerOutput, parseGccStyle, parseJavac, parseNode } from "./diagnostics";

const GCC_SAMPLE = [
  String.raw`C:\Users\sam\AppData\Local\Temp\letters-code-check\4120-3\hello.c: In function 'main':`,
  String.raw`C:\Users\sam\AppData\Local\Temp\letters-code-check\4120-3\hello.c:3:11: error: 'y' undeclared (first use in this function)`,
  String.raw`C:\Users\sam\AppData\Local\Temp\letters-code-check\4120-3\hello.c:3:11: note: each undeclared identifier is reported only once for each function it appears in`,
  String.raw`C:\Users\sam\AppData\Local\Temp\letters-code-check\4120-3\hello.c:4:20: error: expected ';' before 'return'`,
  String.raw`C:\Users\sam\AppData\Local\Temp\letters-code-check\4120-3\hello.c:3:7: warning: unused variable 'x' [-Wunused-variable]`,
].join("\r\n");

const JAVAC_SAMPLE = ["Main.java:3: error: ';' expected", "        int x = 5", "                 ^", "1 error"].join("\n");

const NODE_SAMPLE = [
  String.raw`C:\Users\sam\AppData\Local\Temp\letters-code-check\4120-7\untitled.js:2`,
  "let b = ;",
  "        ^",
  "",
  "SyntaxError: Unexpected token ';'",
  "    at wrapSafe (node:internal/modules/cjs/loader:1900:18)",
  "",
  "Node.js v26.13.0",
].join("\n");

describe("gcc and clang output", () => {
  it("reads errors and warnings with their line and column, keeping a windows drive letter in the path", () => {
    const problems = parseGccStyle(GCC_SAMPLE, "hello.c", "gcc 16.2");
    expect(problems.map((problem) => [problem.line, problem.column, problem.severity])).toEqual([
      [3, 11, "error"],
      [4, 20, "error"],
      [3, 7, "warning"],
    ]);
    expect(problems[0].message).toBe("'y' undeclared (first use in this function)");
    expect(problems[0].source).toBe("gcc 16.2");
  });

  it("leaves out gcc's note about undeclared names being reported once", () => {
    const problems = parseGccStyle(GCC_SAMPLE, "hello.c", "gcc 16.2");
    expect(problems.some((problem) => problem.message.startsWith("each undeclared"))).toBe(false);
  });

  it("puts a problem inside a header on the #include line that brought it in", () => {
    const output = ["In file included from /tmp/check/main.c:2:", "/home/sam/project/util.h:5:1: error: unknown type name 'strin'"].join("\n");
    const problems = parseGccStyle(output, "main.c", "gcc 16.2");
    expect(problems).toEqual([{ line: 2, column: 1, severity: "error", message: "In util.h: unknown type name 'strin'", source: "gcc 16.2" }]);
  });

  it("reads clang's fatal errors", () => {
    const problems = parseGccStyle("/tmp/a/main.c:1:10: fatal error: 'stdioo.h' file not found", "main.c", "clang 18.1");
    expect(problems[0]).toMatchObject({ line: 1, column: 10, severity: "error" });
  });

  it("reads the python check, which prints the same form", () => {
    const problems = parseGccStyle("/tmp/x/untitled.py:2:8: error: expected ':'\n", "untitled.py", "Python 3.12.1");
    expect(problems[0]).toMatchObject({ line: 2, column: 8, message: "expected ':'" });
  });
});

describe("javac output", () => {
  it("takes the column from the caret under the source line", () => {
    const problems = parseJavac(JAVAC_SAMPLE, "Main.java", "javac 21.0.2");
    expect(problems).toEqual([{ line: 3, column: 18, severity: "error", message: "';' expected", source: "javac 21.0.2" }]);
  });
});

describe("node --check output", () => {
  it("reads the line, the caret column and the syntax error", () => {
    const problems = parseNode(NODE_SAMPLE, "untitled.js", "Node.js 26.13.0");
    expect(problems).toEqual([{ line: 2, column: 9, severity: "error", message: "Unexpected token ';'", source: "Node.js 26.13.0" }]);
  });

  it("finds nothing in clean output", () => {
    expect(parseNode("", "untitled.js", "Node.js 26.13.0")).toEqual([]);
  });
});

describe("plain-English hints", () => {
  it("adds a hint under gcc's message for a missing semicolon and an undeclared name", () => {
    const problems = parseCheckerOutput({ format: "gcc", output: GCC_SAMPLE, fileName: "hello.c", checker: "gcc 16.2", language: "c" });
    expect(problems[0].hint).toContain("“y” is used before it is declared");
    expect(problems[1].hint).toContain("semicolon is probably missing");
    expect(problems[1].message).toBe("expected ';' before 'return'");
  });
});
