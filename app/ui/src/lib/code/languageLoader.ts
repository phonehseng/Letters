// loads one language's highlighting and folding for the editor. each
// language package is imported only when a file of that language opens,
// so a window showing HTML never loads the C, Python or SQL grammars.

import type { Extension } from "@codemirror/state";
import type { LanguageId } from "./languages";

export async function loadLanguage(id: LanguageId): Promise<Extension> {
  switch (id) {
    case "html":
      return (await import("@codemirror/lang-html")).html();
    case "css":
      return (await import("@codemirror/lang-css")).css();
    case "javascript":
      return (await import("@codemirror/lang-javascript")).javascript({ jsx: true });
    case "typescript":
      return (await import("@codemirror/lang-javascript")).javascript({ typescript: true, jsx: true });
    case "json":
      return (await import("@codemirror/lang-json")).json();
    case "markdown":
      return (await import("@codemirror/lang-markdown")).markdown();
    case "xml":
      return (await import("@codemirror/lang-xml")).xml();
    case "c":
    case "cpp":
      return (await import("@codemirror/lang-cpp")).cpp();
    case "python":
      return (await import("@codemirror/lang-python")).python();
    case "java":
      return (await import("@codemirror/lang-java")).java();
    case "rust":
      return (await import("@codemirror/lang-rust")).rust();
    case "sql":
      return (await import("@codemirror/lang-sql")).sql();
    case "yaml":
      return (await import("@codemirror/lang-yaml")).yaml();
    // shell and toml have no lezer grammar; codemirror's older stream modes cover them
    case "shell": {
      const [{ StreamLanguage }, { shell }] = await Promise.all([import("@codemirror/language"), import("@codemirror/legacy-modes/mode/shell")]);
      return StreamLanguage.define(shell);
    }
    case "toml": {
      const [{ StreamLanguage }, { toml }] = await Promise.all([import("@codemirror/language"), import("@codemirror/legacy-modes/mode/toml")]);
      return StreamLanguage.define(toml);
    }
    case "text":
      return [];
  }
}
