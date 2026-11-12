// a json check with the exact place and a plain message. a small parser
// of its own (not JSON.parse) because the browser's messages differ
// between versions and only give a character offset.

import type { Problem } from "../problems";
import { problemAt } from "./positions";

export const JSON_SOURCE = "Letters (JSON check)";

class JsonError extends Error {
  constructor(
    readonly offset: number,
    message: string,
  ) {
    super(message);
  }
}

function describe(char: string | undefined): string {
  if (char === undefined) return "the end of the file";
  if (char === "\n") return "a line break";
  return `“${char}”`;
}

class JsonReader {
  index = 0;
  constructor(private readonly text: string) {}

  fail(message: string): never {
    throw new JsonError(this.index, message);
  }

  skipSpace() {
    while (this.index < this.text.length) {
      const char = this.text[this.index];
      if (char === "/" && (this.text[this.index + 1] === "/" || this.text[this.index + 1] === "*")) {
        this.fail("Comments aren’t allowed in JSON.");
      }
      if (!" \t\r\n".includes(char)) return;
      this.index++;
    }
  }

  value() {
    this.skipSpace();
    const char = this.text[this.index];
    if (char === "{") return this.object();
    if (char === "[") return this.array();
    if (char === '"') return this.string();
    if (char === "'") this.fail("JSON strings use double quotes (\"), not single quotes.");
    if (char === "-" || (char >= "0" && char <= "9")) return this.number();
    for (const word of ["true", "false", "null"]) {
      if (this.text.startsWith(word, this.index)) {
        this.index += word.length;
        return;
      }
    }
    this.fail(`Expected a value but found ${describe(char)}.`);
  }

  object() {
    this.index++;
    this.skipSpace();
    if (this.text[this.index] === "}") {
      this.index++;
      return;
    }
    for (;;) {
      this.skipSpace();
      if (this.text[this.index] !== '"') this.fail(`Expected a name in double quotes but found ${describe(this.text[this.index])}.`);
      this.string();
      this.skipSpace();
      if (this.text[this.index] !== ":") this.fail(`Expected “:” after the name but found ${describe(this.text[this.index])}.`);
      this.index++;
      this.value();
      if (this.endOf("}", "object")) return;
    }
  }

  array() {
    this.index++;
    this.skipSpace();
    if (this.text[this.index] === "]") {
      this.index++;
      return;
    }
    for (;;) {
      this.value();
      if (this.endOf("]", "list")) return;
    }
  }

  /** after an item: a comma (more follow) or the closing bracket. true at the end. */
  endOf(closing: string, kind: string): boolean {
    this.skipSpace();
    const char = this.text[this.index];
    if (char === closing) {
      this.index++;
      return true;
    }
    if (char !== ",") this.fail(`Expected “,” or “${closing}” in this ${kind} but found ${describe(char)}.`);
    this.index++;
    this.skipSpace();
    if (this.text[this.index] === closing) this.fail(`A comma can’t come right before “${closing}” in JSON. Remove it.`);
    return false;
  }

  string() {
    this.index++;
    while (this.index < this.text.length) {
      const char = this.text[this.index];
      if (char === '"') {
        this.index++;
        return;
      }
      if (char === "\n") this.fail("This string is missing its closing quote.");
      this.index += char === "\\" ? 2 : 1;
    }
    this.fail("This string is missing its closing quote.");
  }

  number() {
    const match = /^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/.exec(this.text.slice(this.index, this.index + 400));
    if (!match) this.fail("This number isn’t written the way JSON allows.");
    this.index += match[0].length;
  }
}

/** the first problem in a json text, or none. */
export function checkJson(text: string): Problem[] {
  if (text.trim() === "") return [];
  const reader = new JsonReader(text);
  try {
    reader.value();
    reader.skipSpace();
    if (reader.index < text.length) reader.fail(`Unexpected ${describe(text[reader.index])} after the end of the JSON value.`);
    return [];
  } catch (problem) {
    if (!(problem instanceof JsonError)) throw problem;
    return [problemAt(text, problem.offset, 1, "error", problem.message, JSON_SOURCE)];
  }
}
