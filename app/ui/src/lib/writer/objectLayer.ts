// the editor's object layer: decorations that put free objects where the
// model says they are, the invisible floats that make text flow around
// them (wrap.ts), and, in Strict Word mode, the line breaks word chose
// (wordLines.ts). the page surface measures, works these out and hands
// them to the plugin with one transaction that is not part of the undo
// history; they follow edits until the next measurement.

import type { Node as PmNode } from "prosemirror-model";
import { Plugin, PluginKey } from "prosemirror-state";
import { Decoration, DecorationSet } from "prosemirror-view";
import { exclusionStyle, type Exclusion } from "./wrap";

/** where one free object is drawn, in root coordinates. */
export interface Placement {
  pos: number;
  left: number;
  top: number;
  zIndex: number;
  rotation: number;
}

/** a paragraph whose lines break where word breaks them. */
export interface WordLines {
  /** the paragraph's position */
  pos: number;
  /** positions inside it where a new line starts */
  breaks: number[];
}

export interface ObjectLayerState {
  placements: Placement[];
  exclusions: Exclusion[];
  lines: WordLines[];
}

export const objectLayerKey = new PluginKey<ObjectLayerState>("objectLayer");

const EMPTY: ObjectLayerState = { placements: [], exclusions: [], lines: [] };

export function objectLayer(): Plugin<ObjectLayerState> {
  return new Plugin<ObjectLayerState>({
    key: objectLayerKey,
    state: {
      init: () => EMPTY,
      apply: (tr, value) => {
        const next = tr.getMeta(objectLayerKey) as Partial<ObjectLayerState> | undefined;
        if (next) return { ...value, ...next };
        if (!tr.docChanged) return value;
        // until the next measurement, everything follows the edit; a
        // paragraph that was edited loses word's breaks (they no longer fit)
        const map = (pos: number) => tr.mapping.map(pos);
        return {
          placements: value.placements.map((placement) => ({ ...placement, pos: map(placement.pos) })),
          exclusions: value.exclusions,
          lines: value.lines.filter((line) => !touched(tr.mapping, line, tr.before)).map((line) => ({ pos: map(line.pos), breaks: line.breaks.map(map) })),
        };
      },
    },
    props: {
      decorations: (state) => layerDecorations(state.doc, objectLayerKey.getState(state) ?? EMPTY),
    },
  });
}

/** true when an edit changed something inside the paragraph. */
function touched(mapping: { maps: readonly { forEach: (f: (a: number, b: number) => void) => void }[] }, line: WordLines, before: PmNode): boolean {
  const node = before.nodeAt(line.pos);
  const end = line.pos + (node?.nodeSize ?? 0);
  let hit = false;
  for (const step of mapping.maps) step.forEach((from, to) => (hit = hit || (from < end && to > line.pos)));
  return hit;
}

export function layerDecorations(doc: PmNode, layer: ObjectLayerState): DecorationSet {
  const decorations: Decoration[] = [];
  for (const placement of layer.placements) {
    const node = doc.nodeAt(placement.pos);
    if (!node || (node.type.name !== "image" && node.type.name !== "text_box")) continue;
    const transform = placement.rotation ? `;transform:rotate(${placement.rotation}deg)` : "";
    decorations.push(
      Decoration.node(placement.pos, placement.pos + node.nodeSize, {
        class: "w-placed",
        style: `left:${placement.left.toFixed(2)}px;top:${placement.top.toFixed(2)}px;z-index:${placement.zIndex}${transform}`,
      }),
    );
  }
  for (const exclusion of layer.exclusions) {
    const pos = blockStart(doc, exclusion.block);
    if (pos === null) continue;
    decorations.push(Decoration.widget(pos + 1, () => exclusionElement(exclusion), { side: -1, key: `wrap-${exclusion.block}-${exclusionStyle(exclusion)}`, ignoreSelection: true }));
  }
  for (const line of layer.lines) {
    const node = doc.nodeAt(line.pos);
    if (!node?.isTextblock) continue;
    decorations.push(Decoration.node(line.pos, line.pos + node.nodeSize, { class: "w-word-lines" }));
    for (const at of line.breaks) {
      if (at <= line.pos + 1 || at >= line.pos + node.nodeSize - 1) continue;
      decorations.push(Decoration.widget(at, breakElement, { side: -1, key: "word-break", ignoreSelection: true }));
    }
  }
  return DecorationSet.create(doc, decorations);
}

function blockStart(doc: PmNode, index: number): number | null {
  if (index >= doc.childCount) return null;
  let pos = 0;
  for (let i = 0; i < index; i += 1) pos += doc.child(i).nodeSize;
  return doc.child(index).isTextblock ? pos : null;
}

function exclusionElement(exclusion: Exclusion): HTMLElement {
  const element = document.createElement("span");
  element.className = "w-exclusion";
  element.contentEditable = "false";
  element.setAttribute("aria-hidden", "true");
  element.setAttribute("style", exclusionStyle(exclusion));
  return element;
}

function breakElement(): HTMLElement {
  const element = document.createElement("br");
  element.className = "w-word-break";
  return element;
}
