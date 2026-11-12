// the checks that run inside Letters: each finds the mistake a student
// makes most in that language, at the right place, and stays quiet on
// valid files.

import { describe, expect, it } from "vitest";
import { checkBalance, checkPythonIndentation } from "./balance";
import { checkCss } from "./css";
import { checkToml, checkYaml } from "./dataFormats";
import { checkHtml } from "./html";
import { checkJavaScript } from "./javascript";
import { checkJson } from "./json";
import { checkMarkdown, headingAnchor } from "./markdown";
import { checkXml } from "./xml";

const known = (name: string) => ["color", "margin", "display", "background", "width"].includes(name);

describe("json", () => {
  it("gives the exact place of a missing comma", () => {
    const [problem] = checkJson('{\n  "a": 1\n  "b": 2\n}');
    expect(problem).toMatchObject({ line: 3, column: 3, severity: "error" });
    expect(problem.message).toBe("Expected “,” or “}” in this object but found “\"”.");
  });

  it("explains trailing commas and single quotes", () => {
    expect(checkJson('[1, 2, ]')[0].message).toContain("comma can’t come right before");
    expect(checkJson("{'a': 1}")[0].message).toContain("double quotes");
  });

  it("accepts valid json", () => {
    expect(checkJson('{"a": [1, 2.5e3, true, null, "x\\"y"]}')).toEqual([]);
  });
});

describe("html", () => {
  it("finds a tag that is never closed", () => {
    const problems = checkHtml("<body>\n  <div class=\"box\">\n    <p>Hello\n</body>");
    expect(problems.map((problem) => problem.message)).toContain("</body> closes <div> (opened on line 2), which is still open.");
  });

  it("knows void elements and end tags html lets you leave out", () => {
    expect(checkHtml("<ul><li>one<li>two</ul><p>a<p>b<br><img src=x alt=\"\"><input>")).toEqual([]);
  });

  it("reports duplicate ids, missing alt text and a list inside a paragraph", () => {
    const problems = checkHtml('<p id="a">x</p><div id="a"></div><img src="cat.png"><p>text<ul><li>x</li></ul>');
    const messages = problems.map((problem) => problem.message);
    expect(messages).toContain('The id "a" is used more than once (first on line 1).');
    expect(messages).toContain("This picture has no alt text.");
    expect(messages.some((message) => message.startsWith("<ul> can’t go inside <p>"))).toBe(true);
  });

  it("doesn't read tags inside a script", () => {
    expect(checkHtml("<script>if (a < b && c > d) { x = '<div>'; }</script>")).toEqual([]);
  });
});

describe("xml", () => {
  it("is case sensitive and wants one root", () => {
    const messages = checkXml("<?xml version=\"1.0\"?>\n<a><B></b></a><c/>").map((problem) => problem.message);
    expect(messages[0]).toContain("</b> closes <B>");
    expect(messages).toContain("An XML file has one root element. This is a second one.");
  });

  it("accepts well-formed xml with comments and cdata", () => {
    expect(checkXml('<root a="1"><!-- note --><![CDATA[<not a tag>]]><child/></root>')).toEqual([]);
  });
});

describe("css", () => {
  it("finds an unknown property and a missing semicolon", () => {
    const problems = checkCss("p {\n  colour: red;\n  margin: 0\n  display: block;\n}", known);
    expect(problems.map((problem) => [problem.line, problem.message])).toEqual([
      [2, "“colour” isn’t a CSS property. Check the spelling."],
      [3, "A semicolon is missing before “display”."],
    ]);
  });

  it("finds braces that don't match, inside media queries too", () => {
    const problems = checkCss("@media (min-width: 40rem) {\n  p { color: red; }\n", known);
    expect(problems[0].message).toContain("never closed");
  });

  it("reads a ';' inside url(...) as part of the value", () => {
    const icon = ".icon {\n  background: url(data:image/png;base64,iVBORw0KGgo=);\n  color: red;\n}\n";
    expect(checkCss(icon, known)).toEqual([]);
    const lostColon = checkCss(".icon {\n  background url(data:image/png;base64,iVBORw0KGgo=);\n  color: red;\n}\n", known);
    expect(lostColon.map((problem) => [problem.line, problem.message.endsWith("needs a colon between the property and its value.")])).toEqual([[2, true]]);
  });

  it("never flags custom or vendor properties", () => {
    expect(checkCss(":root { --accent: blue; -webkit-appearance: none; }", known)).toEqual([]);
  });
});

describe("javascript", () => {
  it("reports acorn's first syntax error at its place", async () => {
    const [problem] = await checkJavaScript("const a = 1;\nlet b = ;\n");
    expect(problem).toMatchObject({ line: 2, column: 9, severity: "error" });
    expect(problem.message).toBe("Unexpected token");
  });

  it("accepts modules and old scripts alike", async () => {
    expect(await checkJavaScript("import x from './x.js';\nexport const y = () => x ?? 1;")).toEqual([]);
    expect(await checkJavaScript("with (Math) { var r = sqrt(4); }")).toEqual([]);
  });
});

describe("yaml and toml", () => {
  it("reports a yaml mistake with its line", async () => {
    const problems = await checkYaml("name: x\nname: y\n");
    expect(problems[0]).toMatchObject({ line: 2, column: 1, severity: "error", message: "Map keys must be unique" });
    expect((await checkYaml("a: 1\n\tb: 2\n"))[0].message).toBe("Tabs are not allowed as indentation");
  });

  it("reports a toml mistake with its line", async () => {
    const problems = await checkToml('[package]\nname = "letters"\nversion = \n');
    expect(problems[0].line).toBe(3);
    expect(await checkToml('[a]\nb = 1\n')).toEqual([]);
  });
});

describe("markdown", () => {
  it("warns about skipped heading levels, empty links and missing anchors", () => {
    const messages = checkMarkdown("# Title\n### Too Deep\n[here]()\n[jump](#nowhere)\n[ok](#title)\n").map((problem) => problem.message);
    expect(messages).toHaveLength(3);
    expect(messages[0]).toContain("jumps from level 1 to level 3");
  });

  it("ignores links inside code blocks and warns about an open fence", () => {
    const problems = checkMarkdown("```\n[x]()\n");
    expect(problems.map((problem) => problem.message)).toEqual(["This code block is never closed. Add ``` on a line of its own where it ends."]);
  });

  it("makes anchors the way github does", () => {
    expect(headingAnchor("Getting Started, Fast!")).toBe("getting-started-fast");
  });
});

describe("the basic check", () => {
  it("finds a brace that is never closed in c, ignoring braces in strings and comments", () => {
    const problems = checkBalance('int main(void) {\n  printf("{");\n  // }\n  char c = \'{\';\n', "c");
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatchObject({ line: 1, column: 16 });
  });

  it("finds a string missing its closing quote", () => {
    expect(checkBalance('print("hello)\n', "python")[0].message).toContain("missing its closing");
  });

  it("knows rust lifetimes are not strings", () => {
    expect(checkBalance("fn f<'a>(x: &'a str) -> &'a str { x }", "rust")).toEqual([]);
  });

  it("finds python blocks with nothing indented and mixed tabs", () => {
    const problems = checkPythonIndentation("if x:\nprint(x)\ndef f():\n \tpass\n");
    expect(problems.map((problem) => problem.line)).toEqual([1, 4]);
  });

  it("finds a block whose ':' was lost, but lets brackets continue a line", () => {
    const lost = checkPythonIndentation("for n in numbers\n    total += n\n");
    expect(lost.map((problem) => [problem.line, problem.message.startsWith("Unexpected indent")])).toEqual([[2, true]]);
    expect(checkPythonIndentation("total = add(1,\n            2)\n")).toEqual([]);
  });

  it("reads the lines of a python \"\"\" string as text, not as indented code", () => {
    const helpText = 'def help_text():\n    s = """\nUsage: tool [options]\n  -h   show help\n"""\n    return s\n\nprint(help_text())\n';
    expect(checkBalance(helpText, "python")).toEqual([]);
    expect(checkPythonIndentation("def f():\n    '''doc'''\nx = 1\n")).toEqual([]);
    // the same kind of string, but the line after it really is indented too deep
    const indented = checkPythonIndentation('s = """\n  text\n"""\nx = 1\n    y = 2\n');
    expect(indented.map((problem) => [problem.line, problem.message.startsWith("Unexpected indent")])).toEqual([[5, true]]);
  });

  it("ignores brackets and colons inside python strings and comments when reading indentation", () => {
    expect(checkPythonIndentation('x = "(" # [\ny = 1\n')).toEqual([]);
    expect(checkPythonIndentation('if x == ":"\n    y = 1\n')[0].message).toContain("Unexpected indent");
  });
});
