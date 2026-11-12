// the commands of code files, by name, and their keys. they join the app's
// one list in ../commands.ts so the macos menu bar (app/shell/src/menu.rs)
// and the keys reach them through the same dispatcher.
//
// inside the code editor the usual editor keys (Ctrl F, Ctrl G, Ctrl /,
// Alt Z, F8) belong to the editor itself (editorSetup.ts); the two keys
// here work from the viewer and the writer, where those letters are free
// with Ctrl and Alt (Command and Option on a mac).

export const CODE_COMMANDS = [
  // from the viewer and the writer
  "export-code-file",
  "check-document-code",
  // the code editor screen
  "code-go-to-line",
  "code-toggle-comment",
  "code-toggle-wrap",
  "code-toggle-problems",
  "code-next-problem",
  "code-check-now",
  "code-open-in-vscode",
  "code-reveal",
] as const;

export type CodeCommand = (typeof CODE_COMMANDS)[number];

/** read from the physical key, like the other ctrl + alt keys in ../commands.ts. */
export const CODE_KEYS: Record<string, CodeCommand> = {
  // X for "export"
  KeyX: "export-code-file",
  // K for "check code"
  KeyK: "check-document-code",
};
