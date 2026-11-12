import { describe, expect, it } from "vitest";
import {
  commandForKey,
  COMMANDS,
  createDispatcher,
  ECHO_WINDOW_MS,
  isCommand,
  isTextField,
  keyPressOf,
  type KeyPress,
} from "./commands";

const press = (key: string, extra: Partial<KeyPress> = {}): KeyPress => ({
  key,
  code: "",
  ctrlKey: true,
  metaKey: false,
  altKey: false,
  ...extra,
});

describe("commandForKey", () => {
  it("maps the plain shortcuts", () => {
    expect(commandForKey(press("o"))).toBe("open");
    expect(commandForKey(press("N"))).toBe("new-window");
    expect(commandForKey(press("f"))).toBe("find");
    expect(commandForKey(press("="))).toBe("zoom-in");
    expect(commandForKey(press("+"))).toBe("zoom-in");
    expect(commandForKey(press("-"))).toBe("zoom-out");
    expect(commandForKey(press("0"))).toBe("fit-width");
  });

  it("accepts the command key on a mac", () => {
    expect(commandForKey(press("f", { ctrlKey: false, metaKey: true }))).toBe("find");
  });

  it("reads ctrl + alt + digit from the physical key", () => {
    // option + 1 on a mac types ¡, but the key is still Digit1
    expect(commandForKey(press("¡", { altKey: true, code: "Digit1" }))).toBe("toggle-thumbnails");
    expect(commandForKey(press("2", { altKey: true, code: "Digit2" }))).toBe("toggle-outline");
    expect(commandForKey(press("3", { altKey: true, code: "Numpad3" }))).toBe("toggle-two-pages");
  });

  it("still treats AltGr layouts' ctrl + alt + digit as a shortcut outside text fields", () => {
    expect(commandForKey(press("³", { altKey: true, code: "Digit3", altGraph: true }))).toBe("toggle-two-pages");
  });

  it("ignores keys without ctrl, AltGr while typing, or keys not in the list", () => {
    expect(commandForKey(press("f", { ctrlKey: false }))).toBeNull();
    expect(commandForKey(press("³", { altKey: true, code: "Digit3", altGraph: true, typing: true }))).toBeNull();
    expect(commandForKey(press("k"))).toBeNull();
    // alt changes the meaning: ctrl + alt + f is not find
    expect(commandForKey(press("f", { altKey: true, code: "KeyF" }))).toBeNull();
  });
});

describe("commandForKey for document commands", () => {
  it("maps save, save as and rotation", () => {
    expect(commandForKey(press("s"))).toBe("save");
    expect(commandForKey(press("S", { shiftKey: true }))).toBe("save-as");
    expect(commandForKey(press("l"))).toBe("rotate-left");
    expect(commandForKey(press("r"))).toBe("rotate-right");
  });

  it("maps undo and redo, including ctrl + y on windows", () => {
    expect(commandForKey(press("z"))).toBe("undo");
    expect(commandForKey(press("Z", { shiftKey: true }))).toBe("redo");
    expect(commandForKey(press("y"))).toBe("redo");
    expect(commandForKey(press("z", { ctrlKey: false, metaKey: true }))).toBe("undo");
    // command + y is not redo on a mac
    expect(commandForKey(press("y", { ctrlKey: false, metaKey: true }))).toBeNull();
  });

  it("leaves undo and redo to a text field while typing in it", () => {
    expect(commandForKey(press("z", { typing: true }))).toBeNull();
    expect(commandForKey(press("y", { typing: true }))).toBeNull();
    // saving still works from a text field
    expect(commandForKey(press("s", { typing: true }))).toBe("save");
  });
});

describe("commandForKey for page commands", () => {
  const bare = (key: string, extra: Partial<KeyPress> = {}) => press(key, { ctrlKey: false, ...extra });

  it("deletes the selected pages with Delete or Backspace, but never while typing", () => {
    expect(commandForKey(bare("Delete"))).toBe("delete-pages");
    expect(commandForKey(bare("Backspace"))).toBe("delete-pages");
    expect(commandForKey(bare("Delete", { typing: true }))).toBeNull();
    expect(commandForKey(bare("Delete", { shiftKey: true }))).toBeNull();
  });

  it("moves pages with alt and the arrows", () => {
    expect(commandForKey(bare("ArrowUp", { altKey: true }))).toBe("move-pages-up");
    expect(commandForKey(bare("ArrowDown", { altKey: true }))).toBe("move-pages-down");
    expect(commandForKey(bare("ArrowDown"))).toBeNull();
    expect(commandForKey(bare("ArrowDown", { altKey: true, typing: true }))).toBeNull();
  });

  it("opens the tools with ctrl + alt + i, read from the physical key", () => {
    expect(commandForKey(press("ˆ", { altKey: true, code: "KeyI" }))).toBe("toggle-inspector");
  });

  it("leaves plain letters alone", () => {
    expect(commandForKey(bare("d"))).toBeNull();
  });
});

describe("commandForKey for annotations", () => {
  it("marks text and adds comments with ctrl + alt and a letter, read from the physical key", () => {
    expect(commandForKey(press("h", { altKey: true, code: "KeyH" }))).toBe("highlight");
    expect(commandForKey(press("u", { altKey: true, code: "KeyU" }))).toBe("underline");
    expect(commandForKey(press("ß", { altKey: true, code: "KeyS" }))).toBe("strike-through");
    expect(commandForKey(press("n", { altKey: true, code: "KeyN" }))).toBe("add-note");
    expect(commandForKey(press("t", { altKey: true, code: "KeyT" }))).toBe("add-text-box");
    expect(commandForKey(press("d", { altKey: true, code: "KeyD" }))).toBe("draw");
  });

  it("accepts Preview's control + command keys on a mac", () => {
    expect(commandForKey(press("h", { metaKey: true, code: "KeyH" }))).toBe("highlight");
    expect(commandForKey(press("n", { metaKey: true, code: "KeyN" }))).toBe("add-note");
  });

  it("takes no shortcut away from an earlier command", () => {
    // the same letters without alt keep their old meaning
    expect(commandForKey(press("s", { code: "KeyS" }))).toBe("save");
    expect(commandForKey(press("n", { code: "KeyN" }))).toBe("new-window");
    expect(commandForKey(press("ˆ", { altKey: true, code: "KeyI" }))).toBe("toggle-inspector");
    expect(commandForKey(press("1", { altKey: true, code: "Digit1" }))).toBe("toggle-thumbnails");
    // and command + n alone is still a new window on a mac
    expect(commandForKey(press("n", { ctrlKey: false, metaKey: true, code: "KeyN" }))).toBe("new-window");
  });

  it("leaves AltGr letters to a text field", () => {
    expect(commandForKey(press("ś", { altKey: true, code: "KeyS", altGraph: true, typing: true }))).toBeNull();
  });
});

describe("isTextField", () => {
  it("knows which elements undo their own typing", () => {
    expect(isTextField({ tagName: "INPUT" } as Element)).toBe(true);
    expect(isTextField({ tagName: "TEXTAREA" } as Element)).toBe(true);
    expect(isTextField({ tagName: "DIV", isContentEditable: true } as unknown as Element)).toBe(true);
    expect(isTextField({ tagName: "BUTTON", isContentEditable: false } as unknown as Element)).toBe(false);
    expect(isTextField(null)).toBe(false);
  });
});

describe("keyPressOf", () => {
  it("notices when the key was typed into a text field", () => {
    const event = {
      key: "³",
      code: "Digit3",
      ctrlKey: true,
      metaKey: false,
      altKey: true,
      getModifierState: (name: string) => name === "AltGraph",
      target: { tagName: "INPUT", isContentEditable: false },
    } as unknown as KeyboardEvent;
    expect(keyPressOf(event)).toMatchObject({ altGraph: true, typing: true });
    expect(commandForKey(keyPressOf(event))).toBeNull();
  });
});

describe("isCommand", () => {
  it("accepts every menu command and nothing else", () => {
    COMMANDS.forEach((command) => expect(isCommand(command)).toBe(true));
    expect(isCommand("delete-everything")).toBe(false);
    expect(isCommand(3)).toBe(false);
  });
});

describe("createDispatcher", () => {
  it("runs the handler the current screen registered", () => {
    const dispatcher = createDispatcher(() => 0);
    let zoomed = 0;
    dispatcher.register("zoom-in", () => (zoomed += 1));
    expect(dispatcher.dispatch("zoom-in", "keyboard")).toBe(true);
    expect(zoomed).toBe(1);
    expect(dispatcher.dispatch("zoom-out", "keyboard")).toBe(false);
  });

  it("stops running a handler once it is taken back", () => {
    const dispatcher = createDispatcher(() => 0);
    const unregister = dispatcher.register("find", () => {});
    unregister();
    expect(dispatcher.dispatch("find", "menu")).toBe(false);
  });

  it("keeps a newer handler when an older screen unregisters late", () => {
    const dispatcher = createDispatcher(() => 0);
    let calls = "";
    const takeBackOld = dispatcher.register("find", () => (calls += "old"));
    dispatcher.register("find", () => (calls += "new"));
    takeBackOld();
    dispatcher.dispatch("find", "keyboard");
    expect(calls).toBe("new");
  });

  it("drops the menu's echo of a key press, but not a real second press", () => {
    let time = 1000;
    const dispatcher = createDispatcher(() => time);
    let toggles = 0;
    dispatcher.register("toggle-two-pages", () => (toggles += 1));

    dispatcher.dispatch("toggle-two-pages", "keyboard");
    time += 10;
    dispatcher.dispatch("toggle-two-pages", "menu");
    expect(toggles).toBe(1);

    // pressing the keys again on purpose still works
    time += 10;
    dispatcher.dispatch("toggle-two-pages", "keyboard");
    expect(toggles).toBe(2);

    time += ECHO_WINDOW_MS + 1;
    dispatcher.dispatch("toggle-two-pages", "menu");
    expect(toggles).toBe(3);
  });
});
