// every annotation call into the shell, typed, and the shapes of what
// comes back. the shell passes these straight to core/annotate, whose
// rules apply: pages are counted from 0, positions are in points from the
// top left of the page as shown (y down), and every change makes one
// undo step. only the working copy changes until Save.

import { invoke } from "@tauri-apps/api/core";
import type { DocumentState } from "../api";
import type { Point, Rect } from "../pageTools/pagePoints";
import type { PageText } from "../pageTools/textSelection";

/** a colour as the engine stores it, each part 0 to 255. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export type AnnotationKind =
  | "highlight"
  | "underline"
  | "strikeOut"
  | "squiggly"
  | "note"
  | "textBox"
  | "square"
  | "circle"
  | "line"
  | "polygon"
  | "polyLine"
  | "ink"
  | "stamp"
  | "other";

export type TextMarkupKind = "highlight" | "underline" | "strikeOut" | "squiggly";
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
export type TextAlign = "left" | "center" | "right";
export type LineEnding = "none" | "openArrow" | "closedArrow" | "circle" | "square";
export type NoteIcon = "comment" | "note" | "help" | "key" | "insert" | "paragraph" | "newParagraph";

export interface TextBorder {
  color: Rgb;
  width: number;
}

/** what is special about each kind, as the engine reads it back. */
export type AnnotationDetails =
  | { type: "textMarkup"; quads: [Point, Point, Point, Point][] }
  | { type: "note"; icon: NoteIcon; open: boolean; popup: Rect | null }
  | {
      type: "textBox";
      text: string;
      font: StandardFont;
      fontSize: number;
      textColor: Rgb;
      align: TextAlign;
      border: TextBorder | null;
      fill: Rgb | null;
    }
  | { type: "shape"; strokeWidth: number; dash: number[]; fill: Rgb | null }
  | {
      type: "line";
      start: Point;
      end: Point;
      startEnding: LineEnding;
      endEnding: LineEnding;
      strokeWidth: number;
      dash: number[];
      fill: Rgb | null;
    }
  | { type: "polygon"; points: Point[]; closed: boolean; strokeWidth: number; dash: number[]; fill: Rgb | null }
  | { type: "ink"; strokes: Point[][]; width: number }
  | { type: "stamp"; isPicture: boolean }
  | { type: "none" };

export interface AnnotationInfo {
  /** stable across saving and reopening */
  id: string;
  /** counted from 0 */
  page: number;
  kind: AnnotationKind;
  /** the pdf's own name for the kind, such as "Highlight" or "Link" */
  subtype: string;
  rect: Rect;
  color: Rgb | null;
  opacity: number;
  /** the comment */
  contents: string | null;
  author: string | null;
  /** ISO 8601 */
  modified: string | null;
  created: string | null;
  /** the annotation this one answers, in a comment thread */
  replyTo: string | null;
  hidden: boolean;
  /** whether Letters can move and restyle it (it can always be deleted) */
  editable: boolean;
  details: AnnotationDetails;
}

/** what an edit that makes or changes one annotation answers. */
export interface AnnotationEdit {
  document: DocumentState;
  annotation: AnnotationInfo | null;
}

export interface TextMarkupSettings {
  kind: TextMarkupKind;
  /** one per selected line */
  rects: Rect[];
  color: Rgb;
  opacity: number;
  contents?: string | null;
}

export interface NoteSettings {
  /** where the icon's top left corner goes */
  position: Point;
  color: Rgb;
  contents: string;
  icon?: NoteIcon;
}

export interface TextBoxSettings {
  rect: Rect;
  text: string;
  font: StandardFont;
  fontSize: number;
  color: Rgb;
  align: TextAlign;
  border: TextBorder | null;
  fill: Rgb | null;
  opacity: number;
}

export interface StrokeSettings {
  color: Rgb;
  width: number;
  dash: number[];
}

export interface ShapeSettings {
  kind: "square" | "circle";
  rect: Rect;
  stroke: StrokeSettings;
  fill: Rgb | null;
  opacity: number;
}

export interface LineSettings {
  start: Point;
  end: Point;
  stroke: StrokeSettings;
  startEnding: LineEnding;
  endEnding: LineEnding;
  opacity: number;
}

export interface InkSettings {
  strokes: Point[][];
  color: Rgb;
  width: number;
  opacity: number;
}

export interface ImageSettings {
  rect: Rect;
  opacity: number;
}

/**
 * what to change about an annotation. leave a field out to keep it;
 * `fill: null` and `border: null` take the fill or border away.
 */
export interface AnnotationChanges {
  rect?: Rect;
  color?: Rgb;
  fill?: Rgb | null;
  opacity?: number;
  strokeWidth?: number;
  dash?: number[];
  contents?: string | null;
  text?: string;
  font?: StandardFont;
  fontSize?: number;
  align?: TextAlign;
  border?: TextBorder | null;
  points?: Point[];
  strokes?: Point[][];
  startEnding?: LineEnding;
  endEnding?: LineEnding;
}

export type FlattenScope =
  | { scope: "allPages" }
  | { scope: "pages"; pages: number[] }
  | { scope: "annotations"; page: number; ids: string[] };

export interface FlattenEdit {
  document: DocumentState;
  summary: { flattened: number; pages: number[] };
}

export function addTextMarkup(id: number, page: number, params: TextMarkupSettings): Promise<AnnotationEdit> {
  return invoke("add_text_markup", { id, page, params });
}

/** a mark on a selection that runs across pages: one annotation per page, one undo step for all. */
export function addTextMarkups(
  id: number,
  marks: { page: number; params: TextMarkupSettings }[],
): Promise<{ document: DocumentState; annotations: AnnotationInfo[] }> {
  return invoke("add_text_markups", { id, marks });
}

export function addNote(id: number, page: number, params: NoteSettings): Promise<AnnotationEdit> {
  return invoke("add_note", { id, page, params });
}

export function addReply(id: number, page: number, replyTo: string, contents: string): Promise<AnnotationEdit> {
  return invoke("add_reply", { id, page, replyTo, params: { contents } });
}

export function addTextBox(id: number, page: number, params: TextBoxSettings): Promise<AnnotationEdit> {
  return invoke("add_text_box", { id, page, params });
}

export function addShape(id: number, page: number, params: ShapeSettings): Promise<AnnotationEdit> {
  return invoke("add_shape", { id, page, params });
}

export function addLine(id: number, page: number, params: LineSettings): Promise<AnnotationEdit> {
  return invoke("add_line", { id, page, params });
}

export function addInk(id: number, page: number, params: InkSettings): Promise<AnnotationEdit> {
  return invoke("add_ink", { id, page, params });
}

/** the shell reads the picture from `imagePath`; it never travels through the ui. */
export function addImage(id: number, page: number, imagePath: string, params: ImageSettings): Promise<AnnotationEdit> {
  return invoke("add_image", { id, page, imagePath, params: { ...params, image: [] } });
}

export function updateAnnotation(id: number, page: number, annotationId: string, changes: AnnotationChanges): Promise<AnnotationEdit> {
  return invoke("update_annotation", { id, page, annotationId, changes });
}

/** removes the annotation, its pop-up and every reply to it. */
export function deleteAnnotation(id: number, page: number, annotationId: string): Promise<DocumentState> {
  return invoke("delete_annotation", { id, page, annotationId });
}

export function flattenAnnotations(id: number, scope: FlattenScope): Promise<FlattenEdit> {
  return invoke("flatten_annotations", { id, scope });
}

export function listAnnotations(id: number, page: number): Promise<AnnotationInfo[]> {
  return invoke("list_annotations", { id, page });
}

/** every annotation in the document, from one reading of the file. */
export function listAllAnnotations(id: number): Promise<AnnotationInfo[]> {
  return invoke("list_all_annotations", { id });
}

/** the engine's one-sentence explanation of flattening. */
export function flattenExplanation(): Promise<string> {
  return invoke("flatten_explanation");
}

export function pageText(id: number, page: number): Promise<PageText> {
  return invoke("page_text", { id, page });
}

export function pictureSize(path: string): Promise<{ width: number; height: number }> {
  return invoke("picture_size", { path });
}

export function annotationAuthor(): Promise<string> {
  return invoke("annotation_author");
}

/** a blank name goes back to the computer's user name. returns the name now in use. */
export function setAnnotationAuthor(name: string): Promise<string> {
  return invoke("set_annotation_author", { name });
}
