// the slide editor's commands and their keys. the names join the app's
// one list (../commands.ts) so the macos menu bar can send them; the keys
// are read by the slide screen itself, before the window's shortcuts,
// because several of them mean something else on the other screens
// (Ctrl G is Go To in the viewer, Ctrl D is the writer's).
//
// the keys are powerpoint's (windows and mac), keeping those keynote and
// google slides share; where the three disagree the choice is said beside
// the key, and the sources are listed in docs/worklogs/ui-slides.md.

export const SLIDES_COMMANDS = [
  "slides-new-slide",
  "slides-duplicate",
  "slides-duplicate-slide",
  "slides-delete",
  "slides-group",
  "slides-ungroup",
  "slides-bring-to-front",
  "slides-send-to-back",
  "slides-bring-forward",
  "slides-send-backward",
  "slides-play-from-start",
  "slides-play-from-here",
  "slides-presenter-view",
  "slides-replace",
  "slides-toggle-notes",
  "slides-select-all",
  "slides-export-pdf",
] as const;

export type SlidesCommand = (typeof SLIDES_COMMANDS)[number];

export interface SlidesKey {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

/**
 * the command a key press stands for on the slide screen, or null.
 * `mac` picks command over control as the main modifier.
 */
export function slidesCommandForKey(press: SlidesKey, mac: boolean): SlidesCommand | null {
  const main = mac ? press.metaKey : press.ctrlKey;
  if (!main) return functionKey(press, mac);
  const letter = press.code.startsWith("Key") ? press.code.slice(3) : "";
  const { shiftKey: shift, altKey: alt } = press;
  if (letter === "G") return groupKey(shift);
  if (mac) return macKey(press, letter, shift, alt);
  return windowsKey(press, letter, shift, alt);
}

/** group: Ctrl G (powerpoint) or Ctrl Alt G (google slides) on windows, Command Option G on a mac (all three) */
function groupKey(shift: boolean): SlidesCommand {
  return shift ? "slides-ungroup" : "slides-group";
}

function windowsKey(press: SlidesKey, letter: string, shift: boolean, alt: boolean): SlidesCommand | null {
  if (letter === "M" && !shift && !alt) return "slides-new-slide";
  if (letter === "D") return shift ? "slides-duplicate-slide" : "slides-duplicate";
  if (letter === "H" && !shift) return "slides-replace";
  if (letter === "A" && !shift && !alt) return "slides-select-all";
  if (letter === "S" && shift && alt) return "slides-toggle-notes";
  // forward and backward one step: powerpoint's Ctrl Shift ] and [
  if (shift && press.code === "BracketRight") return "slides-bring-forward";
  if (shift && press.code === "BracketLeft") return "slides-send-backward";
  // to the front and back: powerpoint for windows lists none, so google slides' Ctrl Shift ↑ and ↓
  if (shift && press.key === "ArrowUp") return "slides-bring-to-front";
  if (shift && press.key === "ArrowDown") return "slides-send-to-back";
  return null;
}

function macKey(press: SlidesKey, letter: string, shift: boolean, alt: boolean): SlidesCommand | null {
  // new slide: powerpoint and keynote agree on Command Shift N
  if (letter === "N" && shift && !alt) return "slides-new-slide";
  if (letter === "D") return shift ? "slides-duplicate-slide" : "slides-duplicate";
  if (letter === "H" && shift) return "slides-replace";
  if (letter === "A" && !shift && !alt) return "slides-select-all";
  if (letter === "S" && shift && alt) return "slides-toggle-notes";
  // powerpoint for mac and keynote: Command Shift F and B, with Option for one step
  if (letter === "F" && shift) return alt ? "slides-bring-forward" : "slides-bring-to-front";
  if (letter === "B" && shift) return alt ? "slides-send-backward" : "slides-send-to-back";
  if (press.key === "Enter") return shift ? "slides-play-from-start" : "slides-play-from-here";
  return null;
}

/** the show keys without command or control: F5 on windows, Option Return on a mac. */
function functionKey(press: SlidesKey, mac: boolean): SlidesCommand | null {
  if (press.ctrlKey || press.metaKey) return null;
  if (!mac && press.key === "F5") {
    if (press.altKey) return "slides-presenter-view";
    return press.shiftKey ? "slides-play-from-here" : "slides-play-from-start";
  }
  if (mac && press.altKey && press.key === "Enter") return "slides-presenter-view";
  return null;
}

/** what a key does while the slide show runs (the same on every system). */
export type ShowAction = "next" | "previous" | "end" | "black" | "white" | "first" | "last" | { goTo: number } | null;

/**
 * powerpoint's slide show keys: N, Enter, Page Down, → ↓ and Space go on;
 * P, Page Up, ← ↑ and Backspace go back; B or . blacks the screen, W or ,
 * whites it; Esc ends; a number then Enter goes to that slide.
 */
export function showActionForKey(key: string, typedNumber: string): ShowAction {
  if (key === "Enter" && typedNumber !== "") return { goTo: Number(typedNumber) };
  if (["n", "N", "Enter", "PageDown", "ArrowRight", "ArrowDown", " "].includes(key)) return "next";
  if (["p", "P", "PageUp", "ArrowLeft", "ArrowUp", "Backspace"].includes(key)) return "previous";
  if (["b", "B", "."].includes(key)) return "black";
  if (["w", "W", ","].includes(key)) return "white";
  if (key === "Home") return "first";
  if (key === "End") return "last";
  if (key === "Escape" || key === "-") return "end";
  return null;
}

/** the words for a shortcut on this system ("Ctrl M", "⌘⇧N"). */
export function shortcutText(command: SlidesCommand, mac: boolean): string {
  const windows: Partial<Record<SlidesCommand, string>> = {
    "slides-new-slide": "Ctrl M",
    "slides-duplicate": "Ctrl D",
    "slides-duplicate-slide": "Ctrl Shift D",
    "slides-group": "Ctrl G",
    "slides-ungroup": "Ctrl Shift G",
    "slides-bring-to-front": "Ctrl Shift ↑",
    "slides-send-to-back": "Ctrl Shift ↓",
    "slides-bring-forward": "Ctrl Shift ]",
    "slides-send-backward": "Ctrl Shift [",
    "slides-play-from-start": "F5",
    "slides-play-from-here": "Shift F5",
    "slides-presenter-view": "Alt F5",
    "slides-replace": "Ctrl H",
    "slides-toggle-notes": "Ctrl Alt Shift S",
    "slides-delete": "Delete",
  };
  const macs: Partial<Record<SlidesCommand, string>> = {
    "slides-new-slide": "⌘⇧N",
    "slides-duplicate": "⌘D",
    "slides-duplicate-slide": "⌘⇧D",
    "slides-group": "⌥⌘G",
    "slides-ungroup": "⌥⇧⌘G",
    "slides-bring-to-front": "⇧⌘F",
    "slides-send-to-back": "⇧⌘B",
    "slides-bring-forward": "⌥⇧⌘F",
    "slides-send-backward": "⌥⇧⌘B",
    "slides-play-from-start": "⇧⌘↩",
    "slides-play-from-here": "⌘↩",
    "slides-presenter-view": "⌥↩",
    "slides-replace": "⇧⌘H",
    "slides-toggle-notes": "⌥⇧⌘S",
    "slides-delete": "⌫",
  };
  return (mac ? macs : windows)[command] ?? "";
}

export const isMac = () => typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
