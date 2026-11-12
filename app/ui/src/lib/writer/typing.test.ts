// the same cases as the tests in core/office/src/typing.rs, so the
// editor's port and the engine always agree.
import { describe, expect, it } from "vitest";
import {
  applyReplacement,
  defaultTypingSettings,
  onType,
  setToggle,
  toggleForFeature,
  toggleIsOn,
  type TypingSettings,
} from "./typing";

function typed(before: string, c: string, settings: TypingSettings = defaultTypingSettings()): string {
  const change = onType(before, c, settings);
  return change ? applyReplacement(before, change) : `${before}${c}`;
}

describe("typing rules, as in typing.rs", () => {
  it("quotes in english", () => {
    expect(typed("", '"')).toBe("“");
    expect(typed("He said “hi", '"')).toBe("He said “hi”");
    expect(typed("(", '"')).toBe("(“");
    expect(typed("don", "'")).toBe("don’");
    expect(typed("the students", "'")).toBe("the students’");
    expect(typed("a ", "'")).toBe("a ‘");
    expect(typed("“", "'")).toBe("“‘");
    expect(typed("5", "'")).toBe("5′");
    expect(typed("5′10", '"')).toBe("5′10″");
  });

  it("quotes follow the language", () => {
    const settings = { ...defaultTypingSettings(), language: "de-DE" };
    expect(onType("", '"', settings)?.insert).toBe("„");
    expect(onType("Hallo", '"', settings)?.insert).toBe("“");
    settings.language = "fr";
    expect(onType("", '"', settings)?.insert).toBe("« ");
    const close = onType("« oui ", '"', settings);
    expect(close && applyReplacement("« oui ", close)).toBe("« oui »");
  });

  it("sentences start with capitals", () => {
    expect(typed("", "t")).toBe("T");
    expect(typed("It ended. ", "t")).toBe("It ended. T");
    expect(typed("Really? ", "y")).toBe("Really? Y");
    expect(typed("He said “Stop.” ", "t")).toBe("He said “Stop.” T");
    expect(typed("See e.g. ", "t")).toBe("See e.g. t");
    expect(typed("Dr. ", "s")).toBe("Dr. s");
    expect(typed("J. ", "s")).toBe("J. s");
    expect(typed("version 3.", "5")).toBe("version 3.5");
    expect(typed("word", "s")).toBe("words");
  });

  it("autocorrect rules", () => {
    expect(typed("so i", " ")).toBe("so I ");
    expect(typed("so i'm", " ")).toBe("so I’m ");
    expect(typed("THe", " ")).toBe("The ");
    expect(typed("NASA", " ")).toBe("NASA ");
    expect(typed("teh", " ")).toBe("the ");
    expect(typed("Teh", ",")).toBe("The,");
    expect(typed("word-", "-")).toBe("word—");
    expect(typed("this -", " ")).toBe("this – ");
    expect(typed("(c", ")")).toBe("©");
    expect(typed("Brand(TM", ")")).toBe("Brand™");
    expect(typed("wait..", ".")).toBe("wait…");
    expect(typed("a -", ">")).toBe("a →");
  });

  it("every feature can be switched off", () => {
    const settings: TypingSettings = {
      ...defaultTypingSettings(),
      smartQuotes: false,
      capitalizeSentences: false,
      capitalizeI: false,
      fixTwoInitialCapitals: false,
      replaceDashes: false,
      replaceSymbols: false,
      correctSpelling: false,
    };
    for (const [before, c] of [["", '"'], ["", "t"], ["so i", " "], ["teh", " "], ["(c", ")"], ["a-", "-"], ["THe", " "]]) {
      expect(onType(before, c, settings), `${before} ${c}`).toBeNull();
    }
  });

  it("custom rules apply to finished words", () => {
    const settings = { ...defaultTypingSettings(), customRules: [{ from: "lbl", to: "label" }] };
    const change = onType("a lbl", " ", settings);
    expect(change && applyReplacement("a lbl", change)).toBe("a label ");
    expect(change?.feature).toBe("customRules");
  });
});

describe("the four switches people see", () => {
  it("each switch turns its engine features on and off together", () => {
    let settings = defaultTypingSettings();
    expect(toggleIsOn(settings, "autocorrect")).toBe(true);
    settings = setToggle(settings, "autocorrect", false);
    expect(toggleIsOn(settings, "autocorrect")).toBe(false);
    expect(typed("teh", " ", settings)).toBe("teh ");
    expect(typed("word-", "-", settings)).toBe("word--");
    // the others still work
    expect(typed("", '"', settings)).toBe("“");
    settings = setToggle(settings, "capitalize", false);
    expect(typed("", "t", settings)).toBe("t");
    expect(typed("so i", " ", settings)).toBe("so i ");
    settings = setToggle(settings, "smartQuotes", false);
    expect(typed("don", "'", settings)).toBe("don'");
  });

  it("knows which switch made a change", () => {
    expect(toggleForFeature("correctSpelling")).toBe("autocorrect");
    expect(toggleForFeature("capitalizeI")).toBe("capitalize");
    expect(toggleForFeature("smartQuotes")).toBe("smartQuotes");
    expect(toggleForFeature("customRules")).toBeNull();
  });
});
