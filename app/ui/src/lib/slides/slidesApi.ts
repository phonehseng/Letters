// every call from the slide editor into the desktop shell (see
// app/shell/src/slides_commands.rs), with the shapes of what comes back,
// so the editor's components never need to know command names.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog, save as showSaveDialog } from "@tauri-apps/plugin-dialog";
import type { Operation } from "./operations";

export type DeckFormat = "pptx" | "odp";

/** the windows of a slide show's audience start with this, then the deck's id (slides_commands.rs) */
export const AUDIENCE_PREFIX = "window-audience-";

/** .pptx and .odp open in the slide editor; anything the engine can't read falls back to a pdf made by office */
export function isSlidesPath(path: string): boolean {
  return /\.(pptx|odp)$/i.test(path);
}

export const isAudienceWindow = (label: string) => label.startsWith(AUDIENCE_PREFIX);

export interface SlideSummary {
  id: string;
  hidden: boolean;
  layoutId: string | null;
  title: string;
  notes: string;
  transition: string | null;
  animationCount: number;
}

export interface ThemeView {
  id: string;
  name: string;
  colors: { slot: string; hex: string }[];
  majorFont: string;
  minorFont: string;
}

export interface DeckView {
  /** the slide size in points */
  width: number;
  height: number;
  slides: SlideSummary[];
  layouts: { id: string; name: string }[];
  theme: ThemeView | null;
}

export type LineHeight = { type: "multiple"; value: number } | { type: "points"; value: number };

export interface ParagraphView {
  text: string;
  level: number;
  align: "left" | "center" | "right" | "justify";
  marginLeft: number;
  indent: number;
  lineHeight: LineHeight;
  spaceBefore: number;
  spaceAfter: number;
  bullet: string | null;
  font: string;
  size: number;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  color: string;
}

export interface TextView {
  paragraphs: ParagraphView[];
  /** left, top, right, bottom in the shape's own points */
  area: [number, number, number, number];
  anchor: "top" | "middle" | "bottom";
  wrap: boolean;
  fontScale: number;
  prompt: string | null;
}

export interface TableView {
  columns: number[];
  rows: { height: number; cells: { text: string; columnSpan: number; rowSpan: number; covered: boolean }[] }[];
}

export interface ObjectView {
  id: number;
  name: string;
  kind: "shape" | "textBox" | "placeholder" | "picture" | "group" | "connector" | "table" | "chart" | "diagram" | "embedded" | "kept";
  placeholder: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  flipH: boolean;
  flipV: boolean;
  text: TextView | null;
  table: TableView | null;
  kept: string | null;
}

export interface SlideView {
  id: string;
  objects: ObjectView[];
}

export interface HistoryState {
  changedSlides: string[];
  allSlides: boolean;
  canUndo: boolean;
  canRedo: boolean;
  dirty: boolean;
}

export interface OpenedDeck {
  id: number;
  name: string;
  path: string;
  format: DeckFormat;
  deck: DeckView;
  state: HistoryState;
  recovery: { savedAt: number } | null;
}

export interface Edited {
  change: HistoryState;
  deck: DeckView;
  added: number | null;
}

export interface SlidePicture {
  slideId: string;
  document: number;
  revision: number;
  width: number;
  height: number;
  notes: string[];
}

export interface SavedDeck {
  name: string;
  path: string;
  format: DeckFormat;
  notes: string[];
  state: HistoryState;
}

export type Place = { type: "text"; object: number; paragraph: number } | { type: "cell"; object: number; row: number; column: number } | { type: "notes" };

export interface Found {
  slideId: string;
  slideNumber: number;
  place: Place;
  occurrence: number;
  snippet: string;
}

export type LeftOut = { type: "text"; object: number } | { type: "cell"; object: number; row: number; column: number };

export type NewObject = { type: "shape"; preset: string } | { type: "textBox"; text: string } | { type: "table"; rows: number; columns: number };

export interface PointBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type PdfLayout = { type: "slides" } | { type: "notesPages" } | { type: "handouts"; perPage: 2 | 3 | 4 | 6 };

export const openDeck = (path: string, recovered = false) => invoke<OpenedDeck>("slides_open", { path, recovered });
export const closeDeck = (id: number) => invoke<void>("slides_close", { id });
export const deckView = (id: number) => invoke<DeckView>("slides_deck", { id });
export const slideView = (id: number, slideId: string) => invoke<SlideView>("slides_slide", { id, slideId });
export const applyOperation = (id: number, operation: Operation) => invoke<Edited>("slides_apply", { id, operation });
export const setSlideHidden = (id: number, slideId: string, hidden: boolean) => invoke<Edited>("slides_set_hidden", { id, slideId, hidden });
export const duplicateObjects = (id: number, slideId: string, objectIds: number[]) => invoke<Edited>("slides_duplicate", { id, slideId, objectIds });
export const undoEdit = (id: number) => invoke<Edited>("slides_undo", { id });
export const redoEdit = (id: number) => invoke<Edited>("slides_redo", { id });
export const insertObject = (id: number, slideId: string, object: NewObject, at: PointBox) => invoke<Edited>("slides_insert", { id, slideId, object, at });
export const insertPicture = (id: number, slideId: string, path: string, at: PointBox) => invoke<Edited>("slides_insert_picture", { id, slideId, path, at });
export const slidePictures = (id: number, slideIds: string[]) => invoke<SlidePicture[]>("slides_pictures", { id, slideIds });
export const pictureWithout = (id: number, slideId: string, leftOut: LeftOut) => invoke<SlidePicture>("slides_picture_without", { id, slideId, leftOut });
export const findInDeck = (id: number, words: string, matchCase: boolean) => invoke<Found[]>("slides_find", { id, words, matchCase });
export const replaceOne = (id: number, found: Found, words: string, replacement: string, matchCase: boolean) =>
  invoke<Edited>("slides_replace", { id, found, words, replacement, matchCase });
export const saveDeck = (id: number, to: string | null) => invoke<SavedDeck>("slides_save", { id, to });
export const autosaveDeck = (id: number) => invoke<void>("slides_autosave", { id });
export const discardDeckRecovery = (id: number) => invoke<void>("slides_discard_recovery", { id });
export const exportDeckPdf = (id: number, layout: PdfLayout, path: string) => invoke<string[]>("slides_export_pdf", { id, layout, path });
export const screenCount = () => invoke<{ count: number }>("slides_screens").then((screens) => screens.count);
export const setFullScreen = (on: boolean) => invoke<void>("slides_full_screen", { on });
export const openAudienceWindow = (id: number) => invoke<string>("slides_open_audience", { id });
export const closeAudienceWindow = (id: number) => invoke<void>("slides_close_audience", { id });

/** Save As: PowerPoint or OpenDocument, the deck's own format first. */
export async function chooseDeckSaveLocation(suggested: string, format: DeckFormat): Promise<string | null> {
  const filters = [
    { name: "PowerPoint presentation", extensions: ["pptx"] },
    { name: "OpenDocument presentation", extensions: ["odp"] },
  ];
  if (format === "odp") filters.reverse();
  const chosen = await showSaveDialog({ defaultPath: suggested, filters });
  return typeof chosen === "string" ? chosen : null;
}

export async function choosePdfLocation(suggested: string): Promise<string | null> {
  const chosen = await showSaveDialog({ defaultPath: suggested, filters: [{ name: "PDF documents", extensions: ["pdf"] }] });
  return typeof chosen === "string" ? chosen : null;
}

export async function choosePictureFile(): Promise<string | null> {
  const chosen = await showOpenDialog({ multiple: false, filters: [{ name: "Pictures", extensions: ["png", "jpg", "jpeg", "gif", "bmp"] }] });
  return typeof chosen === "string" ? chosen : null;
}
