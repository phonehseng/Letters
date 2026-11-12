// small documents for the writer's tests.
import { EditorState, TextSelection } from "prosemirror-state";
import { modelToDoc } from "./convert";
import { emptyModel, type Block, type DocumentModel } from "./model";

export const para = (text: string, extra: Record<string, unknown> = {}): Block => ({
  type: "paragraph",
  ...extra,
  content: text ? [{ type: "text", text }] : [],
});

export const heading = (level: number, text: string): Block => ({ type: "heading", level, styleId: `Heading${level}`, content: [{ type: "text", text }] });

export function modelWith(blocks: Block[], meta: Partial<DocumentModel> = {}): DocumentModel {
  return { ...emptyModel(), ...meta, blocks };
}

export function stateWith(blocks: Block[], meta: Partial<DocumentModel> = {}): EditorState {
  return EditorState.create({ doc: modelToDoc(modelWith(blocks, meta)) });
}

/** a state with the selection between two positions. */
export function select(state: EditorState, from: number, to = from): EditorState {
  return state.apply(state.tr.setSelection(TextSelection.create(state.doc, from, to)));
}

/** the position just inside the start of top-level block `index`. */
export function startOf(state: EditorState, index: number): number {
  let pos = 0;
  for (let i = 0; i < index; i += 1) pos += state.doc.child(i).nodeSize;
  return pos + 1;
}
