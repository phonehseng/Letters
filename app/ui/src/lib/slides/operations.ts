// the slides engine's editing operations as the shell takes them (json
// tagged by `op`, see core/slides/src/edit/mod.rs). the engine names the
// variants in camelCase but keeps rust's snake_case field names, so the
// fields here are written that way on purpose; a shell-core test parses
// these exact shapes. lengths are emu (12700 to the point), angles
// sixty-thousandths of a degree, as pptx stores them.

export const EMU_PER_POINT = 12700;
const UNITS_PER_DEGREE = 60000;

export type ZOrder = "front" | "back" | "forward" | "backward";

export interface ColorRef {
  kind: { type: "scheme"; slot: string } | { type: "rgb"; hex: string };
}

export type Operation =
  | { op: "addSlide"; index: number; layout_id: string }
  | { op: "duplicateSlide"; index: number }
  | { op: "deleteSlide"; index: number }
  | { op: "moveSlide"; from: number; to: number }
  | { op: "applyLayout"; slide_id: string; layout_id: string }
  | { op: "setText"; slide_id: string; object_id: number; paragraphs: string[] }
  | { op: "deleteObject"; slide_id: string; object_id: number }
  | { op: "move"; slide_id: string; object_ids: number[]; dx: number; dy: number }
  | { op: "resize"; slide_id: string; object_id: number; x: number; y: number; width: number; height: number }
  | { op: "rotate"; slide_id: string; object_id: number; rotation: number }
  | { op: "setFill"; slide_id: string; object_id: number; fill: { type: "solid"; color: ColorRef } | { type: "none" } | null }
  | { op: "reorder"; slide_id: string; object_id: number; to: ZOrder }
  | { op: "group"; slide_id: string; object_ids: number[] }
  | { op: "ungroup"; slide_id: string; group_id: number }
  | { op: "setCellText"; slide_id: string; object_id: number; row: number; column: number; text: string }
  | { op: "insertRow" | "deleteRow" | "insertColumn" | "deleteColumn"; slide_id: string; object_id: number; at: number }
  | { op: "setThemeColor"; theme_id: string; slot: string; color: ColorRef }
  | { op: "setThemeFonts"; theme_id: string; major: string; minor: string }
  | { op: "setNotes"; slide_id: string; text: string }
  | { op: "findReplace"; find: string; replace: string; match_case: boolean };

export const emu = (points: number) => Math.round(points * EMU_PER_POINT);

export function moveBy(slideId: string, objectIds: number[], dxPoints: number, dyPoints: number): Operation {
  return { op: "move", slide_id: slideId, object_ids: objectIds, dx: emu(dxPoints), dy: emu(dyPoints) };
}

export function resizeTo(slideId: string, objectId: number, box: { x: number; y: number; width: number; height: number }): Operation {
  return { op: "resize", slide_id: slideId, object_id: objectId, x: emu(box.x), y: emu(box.y), width: emu(box.width), height: emu(box.height) };
}

/** a rotation in degrees, kept within one turn as the engine stores it. */
export function rotateTo(slideId: string, objectId: number, degrees: number): Operation {
  const turned = ((degrees % 360) + 360) % 360;
  return { op: "rotate", slide_id: slideId, object_id: objectId, rotation: Math.round(turned * UNITS_PER_DEGREE) };
}

export function setText(slideId: string, objectId: number, text: string): Operation {
  return { op: "setText", slide_id: slideId, object_id: objectId, paragraphs: text.split("\n") };
}

export function themeColor(slot: string): ColorRef {
  return { kind: { type: "scheme", slot } };
}

/** "#1F6FEB" or "1f6feb" as a plain colour. */
export function rgbColor(hex: string): ColorRef {
  return { kind: { type: "rgb", hex: hex.replace("#", "").toUpperCase() } };
}
