// the fonts a document uses and replacing one everywhere (fonts.ts)
import { describe, expect, it } from "vitest";
import { docToModel } from "./convert";
import { fontsInText, metricSubstitute, replaceFont } from "./fonts";
import type { DocumentModel } from "./model";
import { stateWith } from "./testDocs";

describe("fonts in the document", () => {
  const state = stateWith(
    [{ type: "paragraph", content: [{ type: "text", text: "Old", marks: [{ type: "fontFamily", name: "Comic Neue" }] }, { type: "text", text: " plain" }] }],
    { styles: { defaults: { run: { font: { name: "Comic Neue" } } }, styles: [{ id: "Normal", name: "Normal", kind: "paragraph", isDefault: true, run: { font: { name: "Comic Neue" } } }] } },
  );

  it("lists the fonts the text sets", () => {
    expect(fontsInText(state.doc)).toEqual(["Comic Neue"]);
  });

  it("Replace changes a font everywhere, text and styles, in one step", () => {
    const tr = replaceFont(state, "Comic Neue", "Arial")!;
    const model = docToModel(tr.doc) as DocumentModel;
    expect(model.blocks[0].content).toEqual([{ type: "text", text: "Old", marks: [{ type: "fontFamily", name: "Arial" }] }, { type: "text", text: " plain" }]);
    expect(model.styles.defaults?.run?.font?.name).toBe("Arial");
    expect(model.styles.styles[0].run?.font?.name).toBe("Arial");
    expect(replaceFont(state, "Not Used", "Arial")).toBeNull();
  });

  it("knows the metric-compatible stand-ins", () => {
    expect(metricSubstitute("Calibri")).toBe("Carlito");
    expect(metricSubstitute("Comic Neue")).toBeNull();
  });
});
