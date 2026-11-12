// every call from the Edit tab into the shell, typed. the shapes mirror
// core/edit (letters-edit), which names fields in camelCase. pages count
// from 0; positions are points from the page's visible top left, y down.
//
// the engine's ids are only valid until the next edit of a page, so every
// change answers the page's objects again with fresh ids. an id that is
// out of date answers `stale: true` (and the fresh objects) instead of an
// error, so the tab can quietly select the same thing again.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";
import type { Point, Rect } from "../pageTools/pagePoints";

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export type FontFamily = "serif" | "sansSerif" | "monospace";

export interface FontInfo {
  id: string;
  /** without a subset tag, e.g. "Arial-BoldMT" */
  name: string;
  family: FontFamily;
  bold: boolean;
  italic: boolean;
  embedded: boolean;
  subset: boolean;
  kind: "type1" | "trueType" | "type3" | "composite";
  /** every character this font can still draw; others need a fallback */
  editableChars: string;
}

export interface TextRun {
  id: string;
  text: string;
  font: FontInfo;
  /** points, as seen on the page */
  size: number;
  color: Rgb;
  charBoxes: Rect[];
  /** invisible text, such as recognized text over a scan */
  invisible: boolean;
}

export interface TextLine {
  id: string;
  rect: Rect;
  text: string;
  baseline: [Point, Point];
  runs: TextRun[];
}

export type TextAlignment = "left" | "right" | "center" | "justified";

export interface TextBlock {
  id: string;
  rect: Rect;
  /** the lines' text joined with new lines */
  text: string;
  lines: TextLine[];
  /** degrees clockwise on screen; 0 runs left to right */
  rotation: number;
  alignment: TextAlignment;
}

export interface ImageObject {
  id: string;
  rect: Rect;
  /** top left, top right, bottom left, bottom right on screen */
  quad: [Point, Point, Point, Point];
  pixelWidth: number;
  pixelHeight: number;
  colorSpace: string;
  bitsPerComponent: number;
  hasAlpha: boolean;
  /** written inside the page's drawing instructions (can't be saved out or replaced) */
  inline: boolean;
  /** "DCTDecode" for a jpeg */
  filter: string | null;
}

export interface PageObjects {
  page: number;
  width: number;
  height: number;
  textBlocks: TextBlock[];
  images: ImageObject[];
}

export interface FontNotice {
  kind: "installedFont" | "standardFont";
  originalFont: string;
  usedFont: string;
  /** the characters the original font couldn't draw */
  characters: string;
  /** the engine's own sentence */
  message: string;
}

export interface TextOverflow {
  linesBefore: number;
  linesAfter: number;
  /** points */
  extraHeight: number;
}

export interface TextEditAnswer {
  document: DocumentState;
  objects: PageObjects;
  notices: FontNotice[];
  overflow: TextOverflow | null;
  stale: boolean;
}

export interface ImageEditAnswer {
  document: DocumentState;
  objects: PageObjects;
  imageId: string | null;
  stale: boolean;
}

export type StandardFont =
  | "helvetica"
  | "helveticaBold"
  | "helveticaOblique"
  | "helveticaBoldOblique"
  | "timesRoman"
  | "timesBold"
  | "timesItalic"
  | "timesBoldItalic"
  | "courier"
  | "courierBold"
  | "courierOblique"
  | "courierBoldOblique";

export interface TextEditOptions {
  /** a block's text is wrapped to its width (true) or kept to the lines typed (false) */
  reflow?: boolean;
  allowInstalledFonts?: boolean;
  allowStandardFonts?: boolean;
}

export interface TextStyleChange {
  font?: StandardFont;
  size?: number;
  color?: Rgb;
}

/** the font new text is written in: a standard one, or a font file from the library */
export type AddTextFont = { type: "standard"; font: StandardFont } | { type: "file"; path: string };

export type ImageFit = "stretch" | "fit" | "fill";

export const editApi = {
  pageObjects: (id: number, page: number) => invoke<PageObjects>("edit_page_objects", { id, page }),
  replaceText: (id: number, page: number, target: string, text: string, fit = false, options?: TextEditOptions) =>
    invoke<TextEditAnswer>("edit_replace_text", { id, page, target, text, fit, options: options ?? null }),
  insertText: (id: number, page: number, lineId: string, index: number, text: string) =>
    invoke<TextEditAnswer>("edit_insert_text", { id, page, lineId, index, text, options: null }),
  deleteChars: (id: number, page: number, lineId: string, start: number, end: number) =>
    invoke<TextEditAnswer>("edit_delete_chars", { id, page, lineId, start, end }),
  deleteText: (id: number, page: number, target: string) => invoke<TextEditAnswer>("edit_delete_text", { id, page, target }),
  setTextStyle: (id: number, page: number, target: string, range: [number, number] | null, style: TextStyleChange) =>
    invoke<TextEditAnswer>("edit_set_text_style", { id, page, target, range, style, options: null }),
  addText: (id: number, page: number, position: Point, text: string, font: AddTextFont, size: number, color: Rgb) =>
    invoke<TextEditAnswer>("edit_add_text", { id, page, added: { position, text, font, size, color } }),
  addImage: (id: number, page: number, rect: Rect, source: { path: string } | { bytes: number[] }) =>
    invoke<ImageEditAnswer>("edit_add_image", {
      id,
      page,
      rect,
      path: "path" in source ? source.path : null,
      bytes: "bytes" in source ? source.bytes : null,
    }),
  replaceImage: (id: number, page: number, imageId: string, path: string, fit: ImageFit) =>
    invoke<ImageEditAnswer>("edit_replace_image", { id, page, imageId, path, fit }),
  moveImage: (id: number, page: number, imageId: string, dx: number, dy: number) =>
    invoke<ImageEditAnswer>("edit_move_image", { id, page, imageId, dx, dy }),
  resizeImage: (id: number, page: number, imageId: string, rect: Rect) =>
    invoke<ImageEditAnswer>("edit_resize_image", { id, page, imageId, rect }),
  rotateImage: (id: number, page: number, imageId: string, degrees: number) =>
    invoke<ImageEditAnswer>("edit_rotate_image", { id, page, imageId, degrees }),
  deleteImage: (id: number, page: number, imageId: string) => invoke<ImageEditAnswer>("edit_delete_image", { id, page, imageId }),
  restackImage: (id: number, page: number, imageId: string, toFront: boolean) =>
    invoke<ImageEditAnswer>("edit_restack_image", { id, page, imageId, toFront }),
  saveImage: (id: number, page: number, imageId: string, path: string) =>
    invoke<string>("edit_save_image", { id, page, imageId, path }),
  /** the embedded font as a font file, when it draws every letter of `sample` (raw bytes) */
  fontFace: (id: number, fontId: string, sample: string) => invoke<ArrayBuffer>("edit_font_face", { id, fontId, sample }),
};
