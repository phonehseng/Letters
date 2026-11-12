import { describe, expect, it } from "vitest";
import {
  choiceAnswer,
  combSpacing,
  editorFontSize,
  editorKind,
  isoDate,
  neighbourWidget,
  startingText,
  textChanged,
  textForEngine,
  toggledAnswer,
  widgetIsOn,
  withinLimit,
} from "./fieldEditing";
import { addFieldEffect, fillEffect, findEffect, flattenEffect, placeSignatureEffect } from "./fillSignEffects";
import type { FieldInfo, SignatureReport } from "./formsApi";
import { newFieldSpec, parseChoices, radioButtonRects, suggestedName } from "./newFields";
import { acceptAll, counts, moveProposal, newFieldsFrom, proposalLabel, proposalsFrom, setKind, setStatus } from "./proposals";
import {
  cleanSignatureText,
  fitInField,
  fitWithin,
  placedAround,
  smoothPath,
  strokesBounds,
  TYPED_STYLES,
  typedCanvasSize,
  whiteToTransparent,
} from "./signaturePicture";
import { afterChoice, bannerText, mustAsk, signatureStatus, signerName, statusSentence } from "./signatures";
import { fieldLabel, formProgress, isFilled, nextEmptyStop, readingOrder, stepStop, stopAnnouncement, tabStops } from "./tabOrder";
import { plainFieldMessage, requiredNote } from "./validation";

/** a field as the engine reports it, with only what a test cares about given. */
function field(name: string, page: number, x: number, y: number, extra: Partial<FieldInfo> = {}): FieldInfo {
  return {
    name,
    kind: "text",
    widgets: [{ page, rect: { x, y, width: 120, height: 20 }, onValue: null }],
    value: null,
    selected: [],
    checked: null,
    defaultValue: null,
    options: [],
    required: false,
    readOnly: false,
    maxLength: null,
    comb: false,
    password: false,
    multiSelect: false,
    editable: false,
    tooltip: null,
    dateFormat: null,
    alignment: "left",
    fontSize: 0,
    signed: false,
    ...extra,
  };
}

function radio(name: string, page: number, values: string[], chosen: string | null = null): FieldInfo {
  return field(name, page, 0, 0, {
    kind: "radioGroup",
    value: chosen,
    widgets: values.map((value, index) => ({ page, rect: { x: 72 + index * 40, y: 300, width: 12, height: 12 }, onValue: value })),
    options: values.map((value) => ({ value, label: value })),
  });
}

describe("tab order", () => {
  // given in tree order, not reading order, as real forms often are
  const fields = [
    field("signature", 1, 72, 600, { kind: "signature" }),
    field("city", 1, 72, 100),
    field("last", 0, 300, 101),
    field("first", 0, 72, 100),
    field("id", 0, 72, 200, { readOnly: true }),
    field("course", 0, 72, 160),
  ];

  it("goes page by page, in rows, top to bottom and left to right, skipping what can't be filled", () => {
    expect(tabStops(fields).map((stop) => stop.field)).toEqual(["first", "last", "course", "city"]);
  });

  it("follows the order the file sets for a page, as Prepare Form › Order writes it", () => {
    // no /Tabs: the page's own list (here "course" first on page 1, then "first", "last")
    const manual = [{ page: 0, kind: null, fields: ["course", "first", "last"] }];
    expect(tabStops(fields, manual).map((stop) => stop.field)).toEqual(["course", "first", "last", "city"]);
    // /Tabs /R: rows, whatever the list says
    expect(tabStops(fields, [{ page: 0, kind: "R", fields: ["course", "first", "last"] }]).map((stop) => stop.field)).toEqual(["first", "last", "course", "city"]);
  });

  it("goes round at the ends, and backwards with shift", () => {
    const stops = tabStops(fields);
    expect(stepStop(stops, "course")?.field).toBe("city");
    expect(stepStop(stops, "city")?.field).toBe("first");
    expect(stepStop(stops, "first", true)?.field).toBe("city");
    expect(stepStop(stops, null)?.field).toBe("first");
    expect(stepStop([], "first")).toBeNull();
  });

  it("makes one stop of a radio group, at its first button", () => {
    const stops = tabStops([radio("year", 0, ["First", "Second", "Third"])]);
    expect(stops).toHaveLength(1);
    expect(stops[0].widget).toBe(0);
  });

  it("puts boxes that overlap by half a line in one row", () => {
    const a = { page: 0, rect: { x: 300, y: 100, width: 50, height: 20 } };
    const b = { page: 0, rect: { x: 72, y: 108, width: 50, height: 20 } };
    expect(readingOrder(a, b)).toBeGreaterThan(0);
    expect(readingOrder({ ...a, page: 0 }, { ...b, page: 1 })).toBeLessThan(0);
  });

  it("finds the next empty field after the current one, going round", () => {
    const filled = [field("a", 0, 72, 100, { value: "Ada" }), field("b", 0, 72, 140), field("c", 1, 72, 100)];
    const stops = tabStops(filled);
    expect(nextEmptyStop(stops, filled, null)?.field).toBe("b");
    expect(nextEmptyStop(stops, filled, "b")?.field).toBe("c");
    expect(nextEmptyStop(stops, filled, "c")?.field).toBe("b");
  });

  it("counts an unticked checkbox as empty only when it is required", () => {
    const boxes = [field("agree", 0, 72, 100, { kind: "checkbox", checked: false }), field("terms", 0, 72, 140, { kind: "checkbox", checked: false, required: true })];
    expect(nextEmptyStop(tabStops(boxes), boxes, null)?.field).toBe("terms");
    const done = boxes.map((box) => ({ ...box, checked: true }));
    expect(nextEmptyStop(tabStops(done), done, null)).toBeNull();
  });

  it("says how many fields are filled", () => {
    const form = [
      field("a", 0, 0, 0, { value: "x" }),
      field("b", 0, 0, 30),
      field("c", 0, 0, 60, { kind: "dropdown", selected: ["Art"] }),
      radio("d", 0, ["Yes", "No"], "No"),
      field("e", 0, 0, 90, { readOnly: true, value: "fixed" }),
    ];
    expect(formProgress(form)).toEqual({ filled: 3, total: 4, text: "3 of 4 filled" });
    expect(isFilled(field("f", 0, 0, 0, { kind: "list", selected: [""] }))).toBe(false);
  });

  it("names a field by its help text, or the last part of its name", () => {
    expect(fieldLabel({ name: "student.id_number", tooltip: null })).toBe("id number");
    expect(fieldLabel({ name: "x", tooltip: " Student ID " })).toBe("Student ID");
  });

  it("tells screen readers where they are", () => {
    const form = [field("name", 0, 72, 100), field("id", 0, 72, 140, { tooltip: "Student ID", required: true }), field("agree", 0, 72, 180, { kind: "checkbox", checked: true })];
    const stops = tabStops(form);
    expect(stopAnnouncement(stops, form[1])).toBe("Field 2 of 3, Student ID, required");
    expect(stopAnnouncement(stops, form[2])).toBe("Field 3 of 3, agree, checkbox, ticked");
  });
});

describe("editing a field", () => {
  it("picks the editor for each kind", () => {
    expect(editorKind(field("a", 0, 0, 0))).toBe("text");
    expect(editorKind(field("a", 0, 0, 0, { comb: true, maxLength: 6 }))).toBe("comb");
    expect(editorKind(field("a", 0, 0, 0, { comb: true }))).toBe("text");
    expect(editorKind(field("a", 0, 0, 0, { kind: "multilineText" }))).toBe("multiline");
    expect(editorKind(field("a", 0, 0, 0, { kind: "date" }))).toBe("date");
    expect(editorKind(field("a", 0, 0, 0, { kind: "list" }))).toBe("list");
    expect(editorKind(field("a", 0, 0, 0, { kind: "checkbox" }))).toBe("toggle");
    expect(editorKind(field("a", 0, 0, 0, { readOnly: true }))).toBe("none");
    expect(editorKind(field("a", 0, 0, 0, { kind: "signature" }))).toBe("none");
  });

  it("starts with the answer, and knows when the typing changed it", () => {
    const answered = field("a", 0, 0, 0, { value: "Ada" });
    expect(startingText(answered)).toBe("Ada");
    expect(textChanged(answered, "Ada")).toBe(false);
    expect(textChanged(answered, "Ada L")).toBe(true);
    expect(startingText(field("b", 0, 0, 0))).toBe("");
  });

  it("cuts typing at the field's limit, counting letters rather than code units", () => {
    expect(withinLimit("12345678", 6)).toBe("123456");
    expect(withinLimit("café", 3)).toBe("caf");
    expect(withinLimit("anything", null)).toBe("anything");
  });

  it("flips a checkbox and chooses a radio button", () => {
    expect(toggledAnswer(field("a", 0, 0, 0, { kind: "checkbox", checked: false }), 0)).toEqual({ kind: "checked", value: true });
    expect(toggledAnswer(field("a", 0, 0, 0, { kind: "checkbox", checked: true }), 0)).toEqual({ kind: "checked", value: false });
    const year = radio("year", 0, ["First", "Second"], "First");
    expect(toggledAnswer(year, 1)).toEqual({ kind: "text", value: "Second" });
    expect(toggledAnswer(year, 0)).toBeNull();
    expect(widgetIsOn(year, 0)).toBe(true);
    expect(widgetIsOn(year, 1)).toBe(false);
  });

  it("moves round a radio group with the arrows", () => {
    const year = radio("year", 0, ["First", "Second", "Third"]);
    expect(neighbourWidget(year, 0, "ArrowRight")).toBe(1);
    expect(neighbourWidget(year, 0, "ArrowUp")).toBe(2);
    expect(neighbourWidget(year, 0, "a")).toBeNull();
  });

  it("chooses from lists", () => {
    expect(choiceAnswer(["Maths", "Art"])).toEqual({ kind: "selected", value: ["Maths", "Art"] });
  });

  it("sizes the editor's text near what the engine draws", () => {
    const rect = { x: 0, y: 0, width: 100, height: 20 };
    expect(editorFontSize(field("a", 0, 0, 0, { fontSize: 9 }), rect)).toBe(9);
    expect(editorFontSize(field("a", 0, 0, 0), rect)).toBeCloseTo(13.91, 2);
    expect(editorFontSize(field("a", 0, 0, 0), { ...rect, height: 100 })).toBe(24);
    expect(editorFontSize(field("a", 0, 0, 0, { kind: "multilineText" }), { ...rect, height: 100 })).toBe(12);
    // a comb's letters also fit their boxes
    expect(editorFontSize(field("a", 0, 0, 0, { comb: true, maxLength: 10 }), { ...rect, height: 30 })).toBeCloseTo(14, 5);
  });

  it("keeps line breaks in a field with several lines, and turns them into spaces in a one-line field", () => {
    expect(textForEngine(field("a", 0, 0, 0, { kind: "multilineText" }), "one\ntwo\r\nthree")).toBe("one\ntwo\nthree");
    expect(textForEngine(field("a", 0, 0, 0), "one\ntwo")).toBe("one two");
    expect(textForEngine(field("a", 0, 0, 0), "one")).toBe("one");
  });

  it("spaces a comb's letters one per box", () => {
    const spacing = combSpacing({ x: 0, y: 0, width: 120, height: 20 }, 6, 10);
    expect(spacing.letterSpacing).toBeCloseTo(14);
    expect(spacing.paddingLeft).toBeCloseTo(7);
  });

  it("writes a picked date the way the engine always accepts", () => {
    expect(isoDate(new Date(2026, 10, 1))).toBe("2026-11-01");
  });
});

describe("validation messages", () => {
  const id = field("student_id", 0, 0, 0, { dateFormat: null });

  it("says the engine's refusals shortly, beside the field", () => {
    expect(plainFieldMessage("The field “student_id” is required, so it can’t be left empty.", id)).toBe("This field is required. Type an answer.");
    expect(plainFieldMessage("The field “student_id” takes at most 8 characters.", id)).toBe("This field takes at most 8 characters.");
    expect(plainFieldMessage("“Jupiter” isn’t one of the choices for “student_id”.", id)).toBe("Choose one of the choices offered.");
    expect(plainFieldMessage("The field “student_id” is read-only, so it can’t be changed.", id)).toBe("This field is read-only, so it can’t be changed.");
    expect(plainFieldMessage("Letters can’t write “日本” into a form field yet. Only letters…", id)).toContain("Western European");
  });

  it("names the date pattern the form expects", () => {
    const due = field("due", 0, 0, 0, { kind: "date", dateFormat: "dd/mm/yyyy" });
    expect(plainFieldMessage("“32/13” isn’t a date Letters understands. Type it like 2026-11-11.", due)).toBe("Type a date as dd/mm/yyyy, or like 2026-11-11.");
  });

  it("keeps any other sentence, with this field for the internal name", () => {
    expect(plainFieldMessage("The field “student_id” can’t take that kind of answer. It takes text.", id)).toBe("The field this field can’t take that kind of answer. It takes text.");
    // form logic's refusals begin with the field's name
    expect(plainFieldMessage("“student_id” takes a number, such as 1,234.50.", id)).toBe("This field takes a number, such as 1,234.50.");
  });

  it("marks required fields without an answer", () => {
    expect(requiredNote(field("a", 0, 0, 0, { required: true }), false)).toBe("Required");
    expect(requiredNote(field("a", 0, 0, 0, { required: true }), true)).toBeNull();
  });
});

describe("Find Fields proposals", () => {
  const found = proposalsFrom([
    {
      page: 0,
      fields: [
        { kind: "text", rect: { x: 100, y: 100, width: 200, height: 18 }, label: "Name:", suggestedName: "Name", maxLength: null, group: null, confidence: 0.9 },
        { kind: "radio", rect: { x: 100, y: 200, width: 10, height: 10 }, label: "Yes", suggestedName: "Attending", maxLength: null, group: 1, confidence: 0.8 },
        { kind: "radio", rect: { x: 160, y: 200, width: 10, height: 10 }, label: "No", suggestedName: "Attending_2", maxLength: null, group: 1, confidence: 0.8 },
        { kind: "comb", rect: { x: 100, y: 300, width: 120, height: 20 }, label: null, suggestedName: "Field", maxLength: 6, group: null, confidence: 0.7 },
      ],
    },
    { page: 1, fields: [{ kind: "checkbox", rect: { x: 72, y: 72, width: 10, height: 10 }, label: null, suggestedName: "Checkbox", maxLength: null, group: null, confidence: 0.5 }] },
  ]);

  it("starts every proposal undecided, with an id per page", () => {
    expect(found.map((proposal) => proposal.id)).toEqual(["0-0", "0-1", "0-2", "0-3", "1-0"]);
    expect(counts(found)).toEqual({ pending: 5, accepted: 0, rejected: 0 });
    expect(newFieldsFrom(found)).toEqual([]);
  });

  it("accepts all but the rejected ones", () => {
    const decided = acceptAll(setStatus(found, "1-0", "rejected"));
    expect(counts(decided)).toEqual({ pending: 0, accepted: 4, rejected: 1 });
  });

  it("accepts a proposal the person moved, but never a rejected one", () => {
    const rect = { x: 90, y: 95, width: 220, height: 20 };
    expect(moveProposal(found, "0-0", rect)[0]).toMatchObject({ rect, status: "accepted" });
    expect(moveProposal(setStatus(found, "0-0", "rejected"), "0-0", rect)[0].status).toBe("rejected");
  });

  it("gives a comb made from another kind a box per square of width", () => {
    expect(setKind(found, "0-0", "comb")[0]).toMatchObject({ kind: "comb", maxLength: 11 });
  });

  it("makes fields of the accepted ones: radio buttons of a group as one field, combs as boxed text", () => {
    const fields = newFieldsFrom(acceptAll(setStatus(found, "1-0", "rejected")));
    expect(fields.map((made) => made.kind.type)).toEqual(["text", "text", "radioGroup"]);
    expect(fields[0]).toMatchObject({ name: "Name", tooltip: "Name", page: 0 });
    expect(fields[1].kind).toEqual({ type: "text", maxLength: 6, comb: true });
    expect(fields[2]).toMatchObject({ name: "Attending", kind: { type: "radioGroup", buttons: [{ value: "Yes" }, { value: "No" }] } });
  });

  it("names a proposal by its label without the colon, or its kind", () => {
    expect(proposalLabel(found[0])).toBe("Name");
    expect(proposalLabel(found[3])).toBe("Boxes");
  });
});

describe("fields added by hand", () => {
  it("gets a name no field has", () => {
    expect(suggestedName("text", ["Text 1", "Text 2"])).toBe("Text 3");
    expect(suggestedName("checkbox", [])).toBe("Checkbox 1");
  });

  it("reads choices one per line, without blanks or repeats", () => {
    expect(parseChoices("Art\n\n Maths \nArt\r\nMusic").map((option) => option.label)).toEqual(["Art", "Maths", "Music"]);
  });

  it("spreads radio buttons along the rectangle drawn", () => {
    const rects = radioButtonRects({ x: 100, y: 100, width: 100, height: 20 }, 3);
    expect(rects.map((rect) => rect.x)).toEqual([100, 142, 184]);
    expect(rects[0]).toMatchObject({ y: 102, width: 16, height: 16 });
  });

  it("asks the engine for the kind drawn", () => {
    const rect = { x: 0, y: 0, width: 100, height: 20 };
    expect(newFieldSpec("radioGroup", 2, rect, "Choice 1", []).kind).toMatchObject({ type: "radioGroup", buttons: [{ value: "Choice 1" }, { value: "Choice 2" }] });
    expect(newFieldSpec("dropdown", 0, rect, "Menu 1", parseChoices("A\nB")).kind).toEqual({ type: "dropdown", options: [{ value: "A", label: "A" }, { value: "B", label: "B" }] });
    expect(newFieldSpec("signature", 0, rect, "Signature 1", []).kind).toEqual({ type: "signature" });
  });
});

describe("signature pictures", () => {
  it("smooths a stroke with curves through its points", () => {
    expect(smoothPath([])).toBe("");
    expect(smoothPath([{ x: 1, y: 1 }])).toBe("M1 1l0.01 0.01");
    expect(smoothPath([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }])).toBe("M0 0L5 0Q10 0 10 5L10 10");
  });

  it("finds the box around the ink, with the pen's width", () => {
    expect(strokesBounds([], 2)).toBeNull();
    expect(strokesBounds([[{ x: 10, y: 10 }, { x: 30, y: 20 }], [{ x: 5, y: 40 }]], 2)).toEqual({ x: 4, y: 9, width: 27, height: 32 });
  });

  it("tidies a typed name and gives its canvas room for the loops", () => {
    expect(cleanSignatureText("  Ada   Lovelace ")).toBe("Ada Lovelace");
    expect(cleanSignatureText("x".repeat(80))).toHaveLength(60);
    expect(typedCanvasSize(300.4, 96)).toEqual({ width: 349, height: 144, x: 24, baseline: 104 });
    expect(TYPED_STYLES.every((style) => style.family.endsWith("cursive") || style.family.endsWith("sans-serif"))).toBe(true);
  });

  it("makes paper white see-through and keeps the ink", () => {
    const pixels = new Uint8ClampedArray([250, 250, 250, 255, 20, 20, 60, 255, 207, 207, 207, 255]);
    whiteToTransparent(pixels);
    // 207 is about half way between keeping (180) and clearing (235): about half its alpha
    expect([pixels[3], pixels[7], pixels[11]]).toEqual([0, 255, 130]);
  });

  it("fits pictures in proportion", () => {
    expect(fitWithin(3000, 1000, 1200, 600)).toEqual({ width: 1200, height: 400 });
    expect(fitWithin(100, 50, 1200, 600)).toEqual({ width: 100, height: 50 });
    expect(fitInField(3, { x: 0, y: 0, width: 180, height: 40 })).toEqual({ x: 30, y: 0, width: 120, height: 40 });
    expect(placedAround({ x: 10, y: 10 }, 3, { width: 612, height: 792 })).toEqual({ x: 0, y: 0, width: 126, height: 42 });
  });
});

function report(extra: Partial<SignatureReport>): SignatureReport {
  return {
    field: "Signature1",
    signer: "Ada Lovelace",
    name: null,
    signedAt: "2026-11-11 10:00 UTC",
    reason: null,
    location: null,
    subFilter: "adbe.pkcs7.detached",
    digestMatches: true,
    signatureCorrect: true,
    valid: true,
    coversWholeFile: true,
    changedAfterSigning: false,
    laterUpdates: 0,
    laterUpdatesAreSignatures: true,
    certificate: { subject: "CN=Ada", issuer: "CN=Ada", commonName: "Ada", selfSigned: true, validFrom: "", validUntil: "", validWhenSigned: true, chain: [] },
    trust: "selfSigned",
    summary: "",
    problems: [],
    ...extra,
  };
}

describe("signatures", () => {
  it("tells valid, changed after signing and damaged apart", () => {
    expect(signatureStatus(report({}))).toBe("valid");
    expect(signatureStatus(report({ changedAfterSigning: true, laterUpdates: 1, laterUpdatesAreSignatures: true }))).toBe("valid");
    expect(signatureStatus(report({ changedAfterSigning: true, laterUpdates: 1, laterUpdatesAreSignatures: false }))).toBe("changed");
    expect(signatureStatus(report({ valid: false, digestMatches: false }))).toBe("changed");
    // saved again whole: the signed part moved, so the signature can't even be read there
    expect(signatureStatus(report({ valid: false, digestMatches: false, certificate: null, changedAfterSigning: true }))).toBe("changed");
    expect(signatureStatus(report({ valid: false, certificate: null }))).toBe("damaged");
  });

  it("says who signed and what the check means", () => {
    expect(signerName(report({ signer: null, name: "A. L." }))).toBe("A. L.");
    expect(signerName(report({ signer: null }))).toBe("Unknown signer");
    expect(statusSentence(report({ valid: false, digestMatches: false }))).toBe("The document has changed since it was signed.");
    expect(statusSentence(report({ valid: false }))).toBe("This signature can’t be checked. The file may be damaged.");
    expect(bannerText([report({})])).toBe("Signed by Ada Lovelace. It hasn’t changed since.");
    expect(bannerText([report({}), report({ valid: false, digestMatches: false })])).toBe("2 signatures. One shows changes or problems.");
    expect(bannerText([])).toBe("");
  });

  it("asks before the first change to a signed document, never for undo or redo", () => {
    const signed = { protected: true, acknowledged: false };
    expect(mustAsk(signed, false)).toBe(true);
    expect(mustAsk(signed, true)).toBe(false);
    expect(mustAsk({ protected: false, acknowledged: false }, false)).toBe(false);
    expect(mustAsk({ protected: true, acknowledged: true }, false)).toBe(false);
  });

  it("goes ahead after Edit Anyway or Save a Copy, and not after Cancel", () => {
    const signed = { protected: true, acknowledged: false };
    expect(afterChoice(signed, "cancel")).toEqual({ proceed: false, saveCopyFirst: false, state: signed });
    expect(afterChoice(signed, "editAnyway")).toEqual({ proceed: true, saveCopyFirst: false, state: { protected: true, acknowledged: true } });
    expect(afterChoice(signed, "saveCopy").saveCopyFirst).toBe(true);
  });
});

describe("what each button will do", () => {
  it("says it plainly", () => {
    expect(fillEffect(0, 0)).toContain("no fields");
    expect(fillEffect(12, 5)).toContain("next of 5 fields without an answer");
    expect(findEffect([], "page", 2, 9)).toContain("on page 3");
    expect(findEffect([], "all", 0, 9)).toContain("all 9 pages");
    expect(findEffect(acceptAll(proposalsFrom([{ page: 0, fields: [] }])), "page", 0, 1)).toContain("on page 1");
    expect(addFieldEffect("Checkbox", false)).toContain("Adds a checkbox field");
    expect(flattenEffect(3, true, 1)).toContain("1 signature field stays a field");
    expect(flattenEffect(0, true, 0)).toBe("There are no form fields to flatten.");
    expect(placeSignatureEffect(false, false)).toContain("Keep Signature");
  });
});
