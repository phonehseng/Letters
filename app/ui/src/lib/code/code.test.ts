// the code screen's pure parts: languages, the problem list, the check
// scheduler, text format labels, code found in documents, exports and
// the line that says who checked the file.

import { describe, expect, it } from "vitest";
import { prepareBlock, repairSwappedCharacters, swappedCharacters } from "./codeBlocks";
import { exportLine, suggestedExportPath } from "./codeExport";
import { checkerLine, resultFromOutcome } from "./compilerCheck";
import { codeFileKind, guessLanguage, isCodePath, languageForPath } from "./languages";
import { countProblems, countsSentence, mergeProblems, nextProblem, type Problem } from "./problems";
import { CheckScheduler, type Timers } from "./scheduler";
import { detectIndentation, encodingLabel, largeFileNotice, type TextFormat } from "./textFormat";

const problem = (line: number, severity: Problem["severity"], message = "m", source = "s"): Problem => ({ line, column: 1, severity, message, source });

describe("languages", () => {
  it("knows a language by its extension and by a few whole names", () => {
    expect(languageForPath("C:\\work\\lab1\\main.c")).toBe("c");
    expect(languageForPath("/home/sam/index.HTML")).toBe("html");
    expect(languageForPath("/home/sam/Makefile")).toBe("shell");
    expect(languageForPath("/home/sam/photo.png")).toBeNull();
    expect(isCodePath("notes.txt")).toBe(true);
    expect(codeFileKind("a.py")).toBe("Python file");
  });

  it("guesses the language of code without a name", () => {
    expect(guessLanguage('#include <stdio.h>\nint main(void) { printf("hi"); }')).toBe("c");
    expect(guessLanguage("#include <iostream>\nint main() { std::cout << 1; }")).toBe("cpp");
    expect(guessLanguage("def add(a, b):\n    return a + b\n")).toBe("python");
    expect(guessLanguage('public class Main { public static void main(String[] a) { System.out.println(1); } }')).toBe("java");
    expect(guessLanguage('{"name": "letters"}')).toBe("json");
    expect(guessLanguage("<!DOCTYPE html>\n<html></html>")).toBe("html");
    expect(guessLanguage("Dear Sam, thank you for the notes.")).toBe("text");
  });
});

describe("the problem list", () => {
  it("prefers the compiler's error on a line the basic check also flagged", () => {
    const merged = mergeProblems([problem(4, "error", "brace", "basic"), problem(9, "warning")], [problem(4, "error", "expected ;", "gcc")]);
    expect(merged.map((each) => each.source)).toEqual(["gcc", "s"]);
  });

  it("counts in words", () => {
    expect(countsSentence(countProblems([problem(1, "error"), problem(2, "error"), problem(3, "warning")]))).toBe("2 errors, 1 warning");
    expect(countsSentence(countProblems([]))).toBe("No problems found");
  });

  it("goes to the next problem and wraps round", () => {
    const list = [problem(3, "error"), problem(10, "warning")];
    expect(nextProblem(list, 3)?.line).toBe(10);
    expect(nextProblem(list, 10)?.line).toBe(3);
    expect(nextProblem(list, 3, true)?.line).toBe(10);
  });
});

/** a clock the test moves by hand. */
function manualTimers() {
  let pending: { run: () => void; at: number } | null = null;
  let now = 0;
  const timers: Timers = {
    set: (run, milliseconds) => (pending = { run, at: now + milliseconds }),
    clear: () => (pending = null),
  };
  const advance = (milliseconds: number) => {
    now += milliseconds;
    if (pending && pending.at <= now) {
      const due = pending;
      pending = null;
      due.run();
    }
  };
  return { timers, advance };
}

describe("the check scheduler", () => {
  it("checks once after typing pauses, not on every key", async () => {
    const { timers, advance } = manualTimers();
    let runs = 0;
    const scheduler = new CheckScheduler(async () => ++runs, () => {}, 400, () => {}, timers);
    scheduler.changed();
    advance(200);
    scheduler.changed();
    advance(200);
    expect(runs).toBe(0);
    advance(200);
    await Promise.resolve();
    expect(runs).toBe(1);
  });

  it("throws away an answer that arrives after a newer check started", async () => {
    const answers: string[] = [];
    const releases: ((value: string) => void)[] = [];
    const scheduler = new CheckScheduler(() => new Promise<string>((resolve) => releases.push(resolve)), (answer) => answers.push(answer), 400);
    const first = scheduler.now();
    const second = scheduler.now();
    releases[1]("new");
    releases[0]("old");
    await Promise.all([first, second]);
    expect(answers).toEqual(["new"]);
  });

  it("delivers nothing and stops the compiler once stopped", async () => {
    let stopped = 0;
    let release: (value: number) => void = () => {};
    const delivered: number[] = [];
    const scheduler = new CheckScheduler(() => new Promise<number>((resolve) => (release = resolve)), (value) => delivered.push(value), 400, () => stopped++);
    const running = scheduler.now();
    scheduler.stop();
    release(1);
    await running;
    expect(delivered).toEqual([]);
    expect(stopped).toBe(1);
  });
});

describe("text format", () => {
  const format: TextFormat = { encoding: "utf8", byteOrderMark: true, lineEnding: "crLf", mixedLineEndings: false, valid: true };

  it("detects tabs, and the usual number of spaces", () => {
    expect(detectIndentation("int f() {\n\treturn 1;\n}\n")).toEqual({ tabs: true, width: 4 });
    expect(detectIndentation("a:\n  b:\n    c: 1\n  d: 2\n")).toEqual({ tabs: false, width: 2 });
    expect(detectIndentation("no indent at all")).toEqual({ tabs: false, width: 4 });
  });

  it("names the encoding and warns about large files", () => {
    expect(encodingLabel(format)).toBe("UTF-8 with BOM");
    expect(largeFileNotice(1000, false)).toBeNull();
    expect(largeFileNotice(20 * 1024 * 1024, false)).toContain("20.0 MB");
    expect(largeFileNotice(40 * 1024 * 1024, true)).toContain("read-only");
  });
});

describe("code found in documents", () => {
  it("flags and repairs the characters documents swap into code", () => {
    const text = "printf(\u201Chello\u201D);\nif (a\u00A0== b) x = 1 \u2013 2;";
    expect(swappedCharacters(text).map((each) => each.line)).toEqual([1, 1, 2, 2]);
    expect(repairSwappedCharacters(text)).toBe('printf("hello");\nif (a == b) x = 1 - 2;');
  });

  it("guesses the language of the repaired text", () => {
    const block = prepareBlock({ text: "def f(x):\n    return x\n", location: "page 2", page: 2, paragraph: null, foundBy: "monospaced font" });
    expect(block.language).toBe("python");
    expect(block.swaps).toEqual([]);
  });
});

describe("exports", () => {
  it("suggests the same name with the format's extension", () => {
    expect(suggestedExportPath("C:\\Essays\\Lab 2.docx", "latex")).toBe("C:\\Essays\\Lab 2.tex");
  });

  it("says exactly what will happen", () => {
    expect(exportLine("markdown", "writer", false)).toBe("Makes a new .md file. Letters reads the document as it is saved. This document doesn’t change.");
    expect(exportLine("csv", "pdf", true)).toContain("Save first");
  });
});

describe("who checked the file", () => {
  it("names the compiler, or says there was only a basic check", () => {
    expect(checkerLine("c", { problems: [], checker: "gcc 16.2", note: null })).toBe("Checked by gcc 16.2");
    expect(checkerLine("c", { problems: [], checker: null, note: "Basic check only: no C compiler (gcc or clang) found on this computer." })).toContain("no C compiler");
    expect(checkerLine("rust", null)).toBe("Basic check only");
    expect(checkerLine("html", null)).toBe("Checked inside Letters");
  });

  it("reads the shell's answer into problems with hints", () => {
    const result = resultFromOutcome("c", {
      checker: "gcc 16.2",
      outputFormat: "gcc",
      output: "/tmp/x/main.c:4:20: error: expected ';' before 'return'\n",
      fileName: "main.c",
      timedOut: false,
      cancelled: false,
      note: null,
    });
    expect(result.problems[0].hint).toContain("semicolon");
  });
});
