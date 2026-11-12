// the codemirror editor the code screen shows: which features it has, its
// colours (from the app's own tokens, so light and dark follow the app),
// and the settings that change while it is open (language, wrapping,
// indentation, read-only), each in its own compartment.

import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab, toggleComment } from "@codemirror/commands";
import { bracketMatching, foldGutter, foldKeymap, HighlightStyle, indentOnInput, indentUnit, syntaxHighlighting } from "@codemirror/language";
import { lintGutter, setDiagnostics, type Diagnostic } from "@codemirror/lint";
import { gotoLine, highlightSelectionMatches, openSearchPanel, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension, type Text } from "@codemirror/state";
import {
  crosshairCursor,
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import type { Problem } from "./problems";
import type { Indentation } from "./textFormat";

/** what the editor tells the screen. */
export interface EditorEvents {
  onChange: (doc: Text) => void;
  onCursor: (line: number, column: number) => void;
  onToggleWrap: () => void;
  onToggleProblems: () => void;
  onNextProblem: (backwards: boolean) => void;
}

export interface EditorOptions {
  text: string;
  /** false for files over the full-features limit: no highlighting, folding or match marks */
  fullFeatures: boolean;
  readOnly: boolean;
  wrap: boolean;
  indentation: Indentation;
  label: string;
  reducedMotion: boolean;
}

export const languageSlot = new Compartment();
export const wrapSlot = new Compartment();
export const indentSlot = new Compartment();
export const readOnlySlot = new Compartment();

// colours are css variables set in CodeEditor.svelte, light and dark, so
// the editor changes with the system without being rebuilt
const highlightStyle = HighlightStyle.define([
  { tag: [tags.keyword, tags.controlKeyword, tags.moduleKeyword, tags.operatorKeyword], color: "var(--code-keyword)" },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], color: "var(--code-string)" },
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: "var(--code-comment)", fontStyle: "italic" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--code-number)" },
  { tag: [tags.typeName, tags.className, tags.namespace], color: "var(--code-type)" },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: "var(--code-function)" },
  { tag: [tags.tagName], color: "var(--code-tag)" },
  { tag: [tags.attributeName, tags.propertyName], color: "var(--code-attribute)" },
  { tag: [tags.processingInstruction, tags.meta, tags.macroName], color: "var(--code-meta)" },
  { tag: tags.heading, fontWeight: "bold", color: "var(--code-keyword)" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strong, fontWeight: "bold" },
  { tag: tags.link, textDecoration: "underline" },
  { tag: tags.invalid, color: "var(--danger)" },
]);

const theme = EditorView.theme({
  "&": { height: "100%", backgroundColor: "var(--code-background)", color: "var(--label)" },
  ".cm-scroller": { fontFamily: "var(--code-font)", fontSize: "var(--code-size)", lineHeight: "1.5" },
  ".cm-gutters": { backgroundColor: "var(--code-background)", color: "var(--label-tertiary)", border: "none" },
  ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--control)" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection": { backgroundColor: "var(--accent-soft) !important" },
  ".cm-cursor": { borderLeftColor: "var(--label)" },
  ".cm-panels": { backgroundColor: "var(--glass)", color: "var(--label)", font: "var(--body)" },
  ".cm-panels-top": { borderBottom: "0.5px solid var(--separator)" },
  ".cm-textfield": { font: "var(--body)", borderRadius: "var(--radius-small)" },
  ".cm-button": { font: "var(--body)", borderRadius: "var(--radius-small)", backgroundImage: "none", backgroundColor: "var(--control)" },
  ".cm-searchMatch": { backgroundColor: "rgb(255 214 10 / 0.35)" },
  ".cm-matchingBracket": { backgroundColor: "var(--accent-soft)", outline: "none" },
});

function indentExtension(indentation: Indentation): Extension {
  const unit = indentation.tabs ? "\t" : " ".repeat(indentation.width);
  return [indentUnit.of(unit), EditorState.tabSize.of(indentation.width)];
}

function codeKeymap(events: EditorEvents): Extension {
  // a key binding says it handled the key by returning true
  const handled = (action: () => void) => () => {
    action();
    return true;
  };
  return keymap.of([
    // the keys vs code uses, so habits carry over
    { key: "Mod-g", run: gotoLine, preventDefault: true },
    { key: "Mod-h", run: openSearchPanel, preventDefault: true },
    { key: "Mod-/", run: toggleComment, preventDefault: true },
    { key: "Alt-z", run: handled(events.onToggleWrap), preventDefault: true },
    { key: "Mod-Shift-m", run: handled(events.onToggleProblems), preventDefault: true },
    { key: "F8", run: handled(() => events.onNextProblem(false)) },
    { key: "Shift-F8", run: handled(() => events.onNextProblem(true)) },
  ]);
}

function listeners(events: EditorEvents): Extension {
  return EditorView.updateListener.of((update) => {
    if (update.docChanged) events.onChange(update.state.doc);
    if (update.selectionSet || update.docChanged) {
      const head = update.state.selection.main.head;
      const line = update.state.doc.lineAt(head);
      events.onCursor(line.number, head - line.from + 1);
    }
  });
}

/** the features only small files get: they cost time and memory on big ones. */
function richFeatures(): Extension {
  return [foldGutter(), indentOnInput(), syntaxHighlighting(highlightStyle), highlightSelectionMatches(), closeBrackets(), keymap.of([...closeBracketsKeymap, ...foldKeymap])];
}

export function createEditor(parent: HTMLElement, options: EditorOptions, events: EditorEvents): EditorView {
  const extensions: Extension[] = [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightSpecialChars(),
    history(),
    // a cursor that doesn't blink when the system asks for less motion
    drawSelection({ cursorBlinkRate: options.reducedMotion ? 0 : 1200 }),
    dropCursor(),
    EditorState.allowMultipleSelections.of(true),
    bracketMatching(),
    rectangularSelection(),
    crosshairCursor(),
    highlightActiveLine(),
    search({ top: true }),
    lintGutter(),
    codeKeymap(events),
    keymap.of([...defaultKeymap, ...searchKeymap, ...historyKeymap, indentWithTab]),
    options.fullFeatures ? richFeatures() : [],
    languageSlot.of([]),
    wrapSlot.of(options.wrap ? EditorView.lineWrapping : []),
    indentSlot.of(indentExtension(options.indentation)),
    readOnlySlot.of(EditorState.readOnly.of(options.readOnly)),
    EditorView.contentAttributes.of({ "aria-label": options.label }),
    theme,
    listeners(events),
  ];
  const state = EditorState.create({ doc: options.text, extensions });
  return new EditorView({ state, parent });
}

export function setWrap(view: EditorView, wrap: boolean) {
  view.dispatch({ effects: wrapSlot.reconfigure(wrap ? EditorView.lineWrapping : []) });
}

export function setIndentation(view: EditorView, indentation: Indentation) {
  view.dispatch({ effects: indentSlot.reconfigure(indentExtension(indentation)) });
}

export function setLanguage(view: EditorView, language: Extension) {
  view.dispatch({ effects: languageSlot.reconfigure(language) });
}

/** the offset of a 1-based line and column, kept inside the document. */
function offsetOf(doc: Text, line: number, column: number): number {
  const safeLine = doc.line(Math.min(Math.max(1, line), doc.lines));
  return Math.min(safeLine.from + Math.max(0, column - 1), safeLine.to);
}

function diagnosticOf(doc: Text, problem: Problem): Diagnostic {
  const from = offsetOf(doc, problem.line, problem.column);
  let to = problem.endLine ? offsetOf(doc, problem.endLine, problem.endColumn ?? problem.column + 1) : from;
  // without an end, the underline covers the word the problem points at
  if (to <= from) to = Math.max(from, wordEnd(doc, from));
  const message = problem.hint ? `${problem.message}\n${problem.hint}` : problem.message;
  return { from, to, severity: problem.severity, message, source: problem.source };
}

function wordEnd(doc: Text, from: number): number {
  const line = doc.lineAt(from);
  const rest = line.text.slice(from - line.from);
  const word = /^[\w$]+/.exec(rest)?.[0].length ?? 1;
  return Math.min(from + word, line.to);
}

/** shows the problems as squiggles and gutter marks. */
export function showProblems(view: EditorView, problems: Problem[]) {
  const diagnostics = problems.map((problem) => diagnosticOf(view.state.doc, problem));
  view.dispatch(setDiagnostics(view.state, diagnostics));
}

/** puts the cursor on a problem and gives the editor the focus. */
export function goTo(view: EditorView, line: number, column: number) {
  const at = offsetOf(view.state.doc, line, column);
  view.dispatch({ selection: { anchor: at }, scrollIntoView: true });
  view.focus();
}
