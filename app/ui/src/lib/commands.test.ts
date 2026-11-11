import { describe, expect, it } from "vitest";
import {
  commandForKey,
  COMMANDS,
  createDispatcher,
  ECHO_WINDOW_MS,
  isCommand,
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
