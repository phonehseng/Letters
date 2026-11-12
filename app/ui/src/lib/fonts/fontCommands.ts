// the font library's command, by name, and its key. it joins the app's
// one command list in ../commands.ts. the library opens over any screen,
// so the command is registered once for the whole window instead of by
// a screen.

import type { Dispatcher } from "../commands";

export const FONT_COMMANDS = ["font-library"] as const;

export type FontCommand = (typeof FONT_COMMANDS)[number];

/**
 * ctrl (command on a mac) and T, the key that shows fonts in Pages,
 * TextEdit and most mac apps. ctrl T means nothing else in Letters.
 */
export function fontKey(press: { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey?: boolean }): FontCommand | null {
  const modifier = press.ctrlKey || press.metaKey;
  if (modifier && !press.altKey && !press.shiftKey && press.key.toLowerCase() === "t") return "font-library";
  return null;
}

/** says what the command does: opens the library (loaded when first used). */
export function registerFontCommands(dispatcher: Pick<Dispatcher, "register">): () => void {
  return dispatcher.register("font-library", () => {
    void import("./openFontLibrary").then(({ openFontLibrary }) => openFontLibrary());
  });
}
