// one list of app commands, and one place that runs them.
//
// a command can arrive two ways: from the menu bar (macOS only; the shell
// sends a "menu-command" event) or from a keyboard shortcut (every system).
// both go through the same dispatcher, so the menu and the keys can never
// drift apart. screens register what each command does while they are shown.

export const COMMANDS = [
  "open",
  "new-window",
  "save",
  "save-as",
  "undo",
  "redo",
  "rotate-left",
  "rotate-right",
  "move-pages-up",
  "move-pages-down",
  "delete-pages",
  "insert-blank-page",
  "insert-from-pdf",
  "extract-pages",
  "split",
  "merge",
  "crop",
  "toggle-inspector",
  "zoom-in",
  "zoom-out",
  "fit-width",
  "find",
  "toggle-thumbnails",
  "toggle-outline",
  "toggle-two-pages",
  "toggle-night-mode",
] as const;

export type Command = (typeof COMMANDS)[number];

export function isCommand(value: unknown): value is Command {
  return typeof value === "string" && (COMMANDS as readonly string[]).includes(value);
}

/** the parts of a keyboard event the shortcuts care about, so tests need no dom. */
export interface KeyPress {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey?: boolean;
  /** true for the AltGr key, which windows reports as ctrl + alt */
  altGraph?: boolean;
  /** true when the focus is in a text field */
  typing?: boolean;
}

// ctrl + alt + a digit. the digit is read from `code` (the physical key)
// because on a mac option + 1 types "¡" and the `key` would not say "1".
const DIGIT_COMMANDS: Record<string, Command> = {
  Digit1: "toggle-thumbnails",
  Numpad1: "toggle-thumbnails",
  Digit2: "toggle-outline",
  Numpad2: "toggle-outline",
  Digit3: "toggle-two-pages",
  Numpad3: "toggle-two-pages",
  // the same keys as Show Inspector in Pages and Keynote
  KeyI: "toggle-inspector",
};

/**
 * the few commands on keys without ctrl or command. they never apply in
 * a text field, where these keys edit text.
 */
function plainKeyCommand(press: KeyPress): Command | null {
  if (press.typing || press.ctrlKey || press.metaKey) return null;
  const noOtherKeys = !press.altKey && !press.shiftKey;
  if (noOtherKeys && (press.key === "Delete" || press.key === "Backspace")) return "delete-pages";
  // alt (option on a mac) + up or down moves the selected pages
  if (press.altKey && !press.shiftKey && press.key === "ArrowUp") return "move-pages-up";
  if (press.altKey && !press.shiftKey && press.key === "ArrowDown") return "move-pages-down";
  return null;
}

/**
 * the command a key press stands for, or null.
 *
 * ctrl on windows and linux, command on a mac. zoom keys are matched
 * by the character typed, so they follow the keyboard layout.
 */
export function commandForKey(press: KeyPress): Command | null {
  if (!(press.ctrlKey || press.metaKey)) return plainKeyCommand(press);
  // on many layouts AltGr + 3 types ³. in a text field the character
  // wins; anywhere else ctrl + alt + 3 is still the shortcut
  if (press.altGraph && press.typing) return null;
  if (press.altKey) return DIGIT_COMMANDS[press.code] ?? null;

  const key = press.key.toLowerCase();
  const history = historyCommand(press, key);
  if (history !== undefined) return history;
  if (key === "s") return press.shiftKey ? "save-as" : "save";
  if (key === "l") return "rotate-left";
  if (key === "r") return "rotate-right";
  if (key === "o") return "open";
  if (key === "n") return "new-window";
  if (key === "f") return "find";
  if (key === "=" || key === "+" || press.code === "NumpadAdd") return "zoom-in";
  if (key === "-" || press.code === "NumpadSubtract") return "zoom-out";
  if (key === "0" || press.code === "Numpad0") return "fit-width";
  return null;
}

/**
 * undo and redo: ctrl (or command) + z, with shift for redo, and ctrl + y
 * for redo as windows apps also accept. returns undefined for other keys.
 *
 * in a text field these keys belong to the field, which undoes its own
 * typing, so null is returned and the browser does what it always does.
 */
function historyCommand(press: KeyPress, key: string): Command | null | undefined {
  // ctrl + y is redo on windows; command + y means nothing on a mac
  const isRedoY = key === "y" && press.ctrlKey && !press.metaKey;
  if (key !== "z" && !isRedoY) return undefined;
  if (press.typing) return null;
  return press.shiftKey || isRedoY ? "redo" : "undo";
}

/** true for elements that keep their own undo history, like text fields. */
export function isTextField(element: Element | null): boolean {
  if (!element) return false;
  const tag = element.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || (element as HTMLElement).isContentEditable === true;
}

/** reads a browser keyboard event into a KeyPress. */
export function keyPressOf(event: KeyboardEvent): KeyPress {
  const target = event.target as HTMLElement | null;
  return {
    key: event.key,
    code: event.code,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    altKey: event.altKey,
    shiftKey: event.shiftKey,
    altGraph: event.getModifierState?.("AltGraph") ?? false,
    typing: isTextField(target),
  };
}

/** where a command came from: the menu bar, a shortcut, or a click in a menu of the window. */
export type CommandSource = "menu" | "keyboard" | "pointer";

export interface Dispatcher {
  /** says what a command does on the current screen. returns a function that takes it back. */
  register: (command: Command, handler: () => void) => () => void;
  /** runs a command. returns false when nothing on this screen handles it. */
  dispatch: (command: Command, source: CommandSource) => boolean;
}

/**
 * if the menu and the keyboard both report the same press (a menu
 * shortcut can reach the page too), the second report within this
 * many milliseconds is dropped, so a toggle is not undone at once.
 */
export const ECHO_WINDOW_MS = 150;

export function createDispatcher(now: () => number = () => performance.now()): Dispatcher {
  const handlers = new Map<Command, () => void>();
  let last: { command: Command; source: CommandSource; at: number } | null = null;

  function register(command: Command, handler: () => void) {
    handlers.set(command, handler);
    return () => {
      // only remove it if a newer screen has not replaced it meanwhile
      if (handlers.get(command) === handler) handlers.delete(command);
    };
  }

  function isEcho(command: Command, source: CommandSource, at: number) {
    return last !== null && last.command === command && last.source !== source && at - last.at < ECHO_WINDOW_MS;
  }

  function dispatch(command: Command, source: CommandSource) {
    const handler = handlers.get(command);
    if (!handler) return false;
    const at = now();
    if (isEcho(command, source, at)) return true;
    last = { command, source, at };
    handler();
    return true;
  }

  return { register, dispatch };
}

/** the one dispatcher of this window. each window runs its own copy of the ui. */
export const appCommands = createDispatcher();
