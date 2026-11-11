// one list of app commands, and one place that runs them.
//
// a command can arrive two ways: from the menu bar (macOS only; the shell
// sends a "menu-command" event) or from a keyboard shortcut (every system).
// both go through the same dispatcher, so the menu and the keys can never
// drift apart. screens register what each command does while they are shown.

export const COMMANDS = [
  "open",
  "new-window",
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
};

/**
 * the command a key press stands for, or null.
 *
 * ctrl on windows and linux, command on a mac. zoom keys are matched
 * by the character typed, so they follow the keyboard layout.
 */
export function commandForKey(press: KeyPress): Command | null {
  if (!(press.ctrlKey || press.metaKey)) return null;
  // on many layouts AltGr + 3 types ³. in a text field the character
  // wins; anywhere else ctrl + alt + 3 is still the shortcut
  if (press.altGraph && press.typing) return null;
  if (press.altKey) return DIGIT_COMMANDS[press.code] ?? null;

  const key = press.key.toLowerCase();
  if (key === "o") return "open";
  if (key === "n") return "new-window";
  if (key === "f") return "find";
  if (key === "=" || key === "+" || press.code === "NumpadAdd") return "zoom-in";
  if (key === "-" || press.code === "NumpadSubtract") return "zoom-out";
  if (key === "0" || press.code === "Numpad0") return "fit-width";
  return null;
}

/** reads a browser keyboard event into a KeyPress. */
export function keyPressOf(event: KeyboardEvent): KeyPress {
  const target = event.target as HTMLElement | null;
  const tag = target?.tagName;
  return {
    key: event.key,
    code: event.code,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    altKey: event.altKey,
    altGraph: event.getModifierState?.("AltGraph") ?? false,
    typing: tag === "INPUT" || tag === "TEXTAREA" || target?.isContentEditable === true,
  };
}

export type CommandSource = "menu" | "keyboard";

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
