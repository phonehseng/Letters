// the slide editor's keys on windows and a mac, the slide show's keys,
// and the operations the editor sends to the shell.

import { describe, expect, it } from "vitest";
import { COMMANDS, isCommand } from "../commands";
import { SLIDES_COMMANDS, showActionForKey, shortcutText, slidesCommandForKey, type SlidesKey } from "./slidesCommands";
import { emu, moveBy, rotateTo, setText } from "./operations";

function press(key: string, code: string, modifiers: Partial<SlidesKey> = {}): SlidesKey {
  return { key, code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...modifiers };
}

describe("the slide editor's keys", () => {
  it("are powerpoint's on windows", () => {
    expect(slidesCommandForKey(press("m", "KeyM", { ctrlKey: true }), false)).toBe("slides-new-slide");
    expect(slidesCommandForKey(press("D", "KeyD", { ctrlKey: true, shiftKey: true }), false)).toBe("slides-duplicate-slide");
    expect(slidesCommandForKey(press("g", "KeyG", { ctrlKey: true }), false)).toBe("slides-group");
    expect(slidesCommandForKey(press("G", "KeyG", { ctrlKey: true, shiftKey: true }), false)).toBe("slides-ungroup");
    expect(slidesCommandForKey(press("F5", "F5"), false)).toBe("slides-play-from-start");
    expect(slidesCommandForKey(press("F5", "F5", { shiftKey: true }), false)).toBe("slides-play-from-here");
    expect(slidesCommandForKey(press("F5", "F5", { altKey: true }), false)).toBe("slides-presenter-view");
    expect(slidesCommandForKey(press("h", "KeyH", { ctrlKey: true }), false)).toBe("slides-replace");
    expect(slidesCommandForKey(press("}", "BracketRight", { ctrlKey: true, shiftKey: true }), false)).toBe("slides-bring-forward");
  });

  it("are powerpoint's and keynote's on a mac", () => {
    expect(slidesCommandForKey(press("N", "KeyN", { metaKey: true, shiftKey: true }), true)).toBe("slides-new-slide");
    expect(slidesCommandForKey(press("g", "KeyG", { metaKey: true, altKey: true }), true)).toBe("slides-group");
    expect(slidesCommandForKey(press("Enter", "Enter", { metaKey: true, shiftKey: true }), true)).toBe("slides-play-from-start");
    expect(slidesCommandForKey(press("Enter", "Enter", { altKey: true }), true)).toBe("slides-presenter-view");
    expect(slidesCommandForKey(press("F", "KeyF", { metaKey: true, shiftKey: true }), true)).toBe("slides-bring-to-front");
    expect(slidesCommandForKey(press("B", "KeyB", { metaKey: true, shiftKey: true, altKey: true }), true)).toBe("slides-send-backward");
    // control alone is not the mac's main key
    expect(slidesCommandForKey(press("m", "KeyM", { ctrlKey: true }), true)).toBeNull();
  });

  it("leave other keys to the window", () => {
    expect(slidesCommandForKey(press("s", "KeyS", { ctrlKey: true }), false)).toBeNull();
    expect(slidesCommandForKey(press("a", "KeyA"), false)).toBeNull();
  });

  it("drive the slide show as powerpoint's do", () => {
    expect(showActionForKey(" ", "")).toBe("next");
    expect(showActionForKey("PageUp", "")).toBe("previous");
    expect(showActionForKey("b", "")).toBe("black");
    expect(showActionForKey(",", "")).toBe("white");
    expect(showActionForKey("Enter", "12")).toEqual({ goTo: 12 });
    expect(showActionForKey("Escape", "")).toBe("end");
  });

  it("are all in the app's one list, with words for each system", () => {
    for (const command of SLIDES_COMMANDS) expect(isCommand(command)).toBe(true);
    expect(COMMANDS).toContain("slides-new-slide");
    expect(shortcutText("slides-new-slide", false)).toBe("Ctrl M");
    expect(shortcutText("slides-new-slide", true)).toBe("⌘⇧N");
  });
});

describe("the operations the editor sends", () => {
  it("are in emu and sixty-thousandths of a degree, with snake_case fields", () => {
    expect(emu(1)).toBe(12700);
    expect(moveBy("s", [3], 10, -2)).toEqual({ op: "move", slide_id: "s", object_ids: [3], dx: 127000, dy: -25400 });
    expect(rotateTo("s", 3, -90)).toEqual({ op: "rotate", slide_id: "s", object_id: 3, rotation: 270 * 60000 });
    expect(setText("s", 3, "a\nb")).toEqual({ op: "setText", slide_id: "s", object_id: 3, paragraphs: ["a", "b"] });
  });
});
