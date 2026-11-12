// the Edit tab's round-two calls into the shell, typed: text that reflows
// (paragraphs and table cells), text in the page's style, shapes, picture
// crops, page setup, and the signature check. the shapes mirror core/edit
// (letters-edit), which names fields in camelCase. pages count from 0;
// positions are points from the page's visible top left, y down.
//
// every change takes the person's consent for signed files (see
// consent.svelte.ts) and answers the page's objects again with fresh ids.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";
import type { Point, Rect } from "../pageTools/pagePoints";
import type { FontNotice, PageObjects, Rgb } from "./editApi";

/** the person agreed that editing makes the file's digital signatures invalid. */
export interface EditConsent {
  invalidateSignatures: boolean;
}

export interface SignatureInfo {
  field: string;
  signer: string | null;
  signedAt: string | null;
}

export interface EditCheck {
  locked: boolean;
  signatures: SignatureInfo[];
  /** the engine's own sentence to show and confirm */
  message: string | null;
}

/** letters added to a font already in the file, and how they were made. */
export interface FontExtension {
  font: string;
  characters: string;
  /** e.g. "Arial on this computer" */
  glyphSource: string;
  /** "same font", "adjusted near font", "built from the font’s own glyphs", "look-alike" */
  matchMethod: string;
  /** percent, the least alike new letter */
  similarity: number | null;
  /** one sentence for the person */
  sentence: string;
}

export interface Tightening {
  letterSpacingEm: number;
  wordSpacingScale: number;
  leadingScale: number;
}

/** everything an edit changed besides the thing edited. */
export interface ChangesSummary {
  pagesChanged: number[];
  textLinesMoved: number;
  imagesMoved: number;
  pathsMoved: number;
  annotationsMoved: number;
  pathsReshaped: number;
  fontsExtended: FontExtension[];
  tightening: Tightening | null;
  notes: string[];
}

export type OverflowChoice = "ask" | "allowIntoMargin" | "tighten" | "carryToNextPage" | "leave";

export type LimitReason = "caller" | "obstacle" | "footer" | "bottomMargin";

export interface OverflowOption {
  choice: OverflowChoice;
  possible: boolean;
  /** the engine's sentence: what it would do, or why it can't */
  detail: string;
}

export interface OverflowReport {
  /** points past the limit */
  amount: number;
  bottomLimit: number;
  limitReason: LimitReason;
  choices: OverflowOption[];
}

export interface ReflowOptions {
  overflow?: OverflowChoice;
  consent?: EditConsent;
}

export interface FlowAnswer {
  document: DocumentState;
  objects: PageObjects;
  /** false when nothing changed because the person must choose (see overflow) */
  applied: boolean;
  notices: FontNotice[];
  summary: ChangesSummary;
  blockId: string | null;
  linesBefore: number;
  linesAfter: number;
  heightChange: number;
  overflow: OverflowReport | null;
  inTable: boolean;
  stale: boolean;
}

export interface ContentAnswer {
  document: DocumentState;
  objects: PageObjects;
  notices: FontNotice[];
  summary: ChangesSummary;
  objectIds: string[];
  stale: boolean;
}

/** a new colour for a shape's fill or outline. */
export type NewPaint = { type: "none" } | { type: "rgb"; color: Rgb } | { type: "gray"; value: number };

export interface PathStyle {
  fill?: NewPaint;
  stroke?: NewPaint;
  strokeWidth?: number;
  /** dash lengths in points; empty for a solid line */
  dash?: number[];
  opacity?: number;
}

export type FlipDirection = "horizontal" | "vertical";

export interface Margins {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** [left, bottom, right, top] in pdf points, before the page's rotation */
export type BoxNumbers = [number, number, number, number];

export interface PageBoxes {
  media: BoxNumbers | null;
  crop: BoxNumbers | null;
  bleed: BoxNumbers | null;
  trim: BoxNumbers | null;
  art: BoxNumbers | null;
}

export interface PageLayout {
  margins: { page: number; width: number; height: number; content: Rect | null; margins: Margins };
  boxes: PageBoxes;
}

export interface PageAnswer {
  document: DocumentState;
  layout: PageLayout;
  summary: ChangesSummary;
}

export type MarginFit = "scale" | "shift";
export type PaperFit = "fit" | "fill" | "keepSize";
export type Side = "top" | "right" | "bottom" | "left";

/** new text in the style of the paragraph nearby */
export interface MatchedText {
  position: Point;
  text: string;
  width: number | null;
  likeBlock: string | null;
}

type Consent = EditConsent;

export const contentApi = {
  check: (id: number) => invoke<EditCheck>("edit_check", { id }),
  reflowText: (id: number, page: number, blockId: string, text: string, options: ReflowOptions) =>
    invoke<FlowAnswer>("edit_reflow_text", { id, page, blockId, text, options }),
  addMatchingText: (id: number, page: number, added: MatchedText, consent: Consent) =>
    invoke<ContentAnswer>("edit_add_matching_text", { id, page, added, consent }),

  movePaths: (id: number, page: number, ids: string[], dx: number, dy: number, consent: Consent) =>
    invoke<ContentAnswer>("edit_move_paths", { id, page, ids, dx, dy, consent }),
  resizePaths: (id: number, page: number, ids: string[], rect: Rect, consent: Consent) =>
    invoke<ContentAnswer>("edit_resize_paths", { id, page, ids, rect, consent }),
  rotatePaths: (id: number, page: number, ids: string[], degrees: number, consent: Consent) =>
    invoke<ContentAnswer>("edit_rotate_paths", { id, page, ids, degrees, consent }),
  stylePaths: (id: number, page: number, ids: string[], style: PathStyle, consent: Consent) =>
    invoke<ContentAnswer>("edit_style_paths", { id, page, ids, style, consent }),
  pathPoints: (id: number, page: number, pathId: string, points: Point[], consent: Consent) =>
    invoke<ContentAnswer>("edit_path_points", { id, page, pathId, points, consent }),
  deletePaths: (id: number, page: number, ids: string[], consent: Consent) =>
    invoke<ContentAnswer>("edit_delete_paths", { id, page, ids, consent }),
  restackPaths: (id: number, page: number, ids: string[], toFront: boolean, consent: Consent) =>
    invoke<ContentAnswer>("edit_restack_paths", { id, page, ids, toFront, consent }),
  duplicatePaths: (id: number, page: number, ids: string[], consent: Consent) =>
    invoke<ContentAnswer>("edit_duplicate_paths", { id, page, ids, consent }),

  cropImage: (id: number, page: number, imageId: string, rect: Rect, consent: Consent) =>
    invoke<ContentAnswer>("edit_crop_image", { id, page, imageId, rect, consent }),
  flipImage: (id: number, page: number, imageId: string, direction: FlipDirection, consent: Consent) =>
    invoke<ContentAnswer>("edit_flip_image", { id, page, imageId, direction, consent }),
  imageOpacity: (id: number, page: number, imageId: string, opacity: number, consent: Consent) =>
    invoke<ContentAnswer>("edit_image_opacity", { id, page, imageId, opacity, consent }),

  pageLayout: (id: number, page: number) => invoke<PageLayout>("edit_page_layout", { id, page }),
  setMargins: (id: number, page: number, margins: Margins, fit: MarginFit, consent: Consent) =>
    invoke<PageAnswer>("edit_set_margins", { id, page, margins, fit, consent }),
  resizePage: (id: number, page: number, width: number, height: number, fit: PaperFit, consent: Consent) =>
    invoke<PageAnswer>("edit_resize_page", { id, page, width, height, fit, consent }),
  addPageSpace: (id: number, page: number, side: Side, points: number, consent: Consent) =>
    invoke<PageAnswer>("edit_add_page_space", { id, page, side, points, consent }),
  setPageBoxes: (id: number, page: number, boxes: Partial<PageBoxes>, consent: Consent) =>
    invoke<PageAnswer>("edit_set_page_boxes", { id, page, boxes, consent }),
};
