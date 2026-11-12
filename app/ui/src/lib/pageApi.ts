// every page tool call into the shell, typed, and the pickers they need.
//
// pages are counted from 0. every tool that changes the document returns
// its new state; only the working copy changes until Save. tools that
// make other files (extract, split, merge) return what they wrote and
// leave the open document alone.

import { invoke } from "@tauri-apps/api/core";
import { open as showOpenDialog } from "@tauri-apps/plugin-dialog";
import type { DocumentState } from "./api";

/** turns pages by quarter turns: 1 clockwise, -1 anticlockwise, 2 upside down. */
export function rotatePages(id: number, pages: number[], quarterTurns: number): Promise<DocumentState> {
  return invoke("rotate_pages", { id, pages, quarterTurns });
}

/** moves pages, as a block, to just before the page now at `beforeIndex` (the page count means the end). */
export function movePages(id: number, pages: number[], beforeIndex: number): Promise<DocumentState> {
  return invoke("move_pages", { id, pages, beforeIndex });
}

export function deletePages(id: number, pages: number[]): Promise<DocumentState> {
  return invoke("delete_pages", { id, pages });
}

export interface InsertBlankSettings {
  /** the new pages go before the page now at this index */
  atIndex: number;
  widthPt: number;
  heightPt: number;
  count: number;
}

export function insertBlankPages(id: number, params: InsertBlankSettings): Promise<DocumentState> {
  return invoke("insert_blank_pages", { id, params });
}

/** copies pages of another pdf in; with no `otherPages` every page comes. */
export function insertPagesFrom(
  id: number,
  atIndex: number,
  otherPath: string,
  otherPages: number[] | null,
): Promise<DocumentState> {
  return invoke("insert_pages_from", { id, atIndex, otherPath, otherPages });
}

/** a rectangle in points from the top left of the page as it is shown. */
export interface CropSettings {
  pages: number[];
  rect: { left: number; top: number; width: number; height: number };
}

export function cropPages(id: number, params: CropSettings): Promise<DocumentState> {
  return invoke("crop_pages", { id, params });
}

export function uncropPages(id: number, pages: number[]): Promise<DocumentState> {
  return invoke("uncrop_pages", { id, params: { pages } });
}

export type VerticalEdge = "top" | "bottom";
export type HorizontalPlace = "left" | "center" | "right";
export type NumberFormat = "number" | "pageWord" | "ofTotal" | "roman";
export type StampLayer = "inFront" | "behind";
export type StampKind = "pageNumbers" | "headerFooter" | "watermark";

/** settings for page numbers. fields left out take the engine's defaults. */
export interface PageNumberSettings {
  position: { vertical: VerticalEdge; horizontal: HorizontalPlace };
  startNumber: number;
  format: NumberFormat;
  skipFirstPage: boolean;
  /** null numbers every page */
  pages: number[] | null;
}

export function addPageNumbers(id: number, params: PageNumberSettings): Promise<DocumentState> {
  return invoke("add_page_numbers", { id, params });
}

/** each text may use {page}, {pages}, {date} and {file}. */
export interface HeaderFooterSettings {
  edge: VerticalEdge;
  left: string;
  center: string;
  right: string;
  pages: number[] | null;
  /** what {file} becomes */
  fileName: string;
  /** what {date} becomes */
  date: string;
}

export function addHeaderFooter(id: number, params: HeaderFooterSettings): Promise<DocumentState> {
  return invoke("add_header_footer", { id, params });
}

export interface TextWatermarkSettings {
  text: string;
  /** 0 is invisible, 1 is solid */
  opacity: number;
  layer: StampLayer;
  pages: number[] | null;
}

export function addTextWatermark(id: number, params: TextWatermarkSettings): Promise<DocumentState> {
  return invoke("add_text_watermark", { id, params });
}

export interface ImageWatermarkSettings {
  opacity: number;
  /** 1 makes the picture as big as fits on the page */
  scale: number;
  layer: StampLayer;
  pages: number[] | null;
}

/** the picture is read from `imagePath` by the shell. */
export function addImageWatermark(id: number, imagePath: string, params: ImageWatermarkSettings): Promise<DocumentState> {
  return invoke("add_image_watermark", { id, imagePath, params });
}

export function removeStamps(id: number, kind: StampKind, pages: number[] | null): Promise<DocumentState> {
  return invoke("remove_stamps", { id, params: { kind, pages } });
}

/** saves copies of pages as a new pdf. returns the file written. */
export function extractPages(id: number, pages: number[], savePath: string): Promise<string> {
  return invoke("extract_pages", { id, pages, savePath });
}

export type SplitSettings =
  | { mode: "byRanges"; ranges: { start: number; end: number }[] }
  | { mode: "everyNPages"; pagesPerPart: number }
  | { mode: "byBookmarks" }
  | { mode: "byMaxSize"; maxBytes: number };

export interface SplitFile {
  path: string;
  name: string;
  firstPage: number;
  pageCount: number;
  /** a single page was already bigger than the size asked for */
  overSizeLimit: boolean;
}

/** splits into new files in `folder`; existing files are never replaced. */
export function splitDocument(id: number, params: SplitSettings, folder: string, baseName: string): Promise<SplitFile[]> {
  return invoke("split_document", { id, params, folder, baseName });
}

/** combines whole pdfs, in order, into a new file. returns the file written. */
export function mergeDocuments(paths: string[], names: string[], savePath: string): Promise<string> {
  return invoke("merge_documents", { paths, names, savePath });
}

/** the number of pages of another pdf, or the reason it can't be used. */
export function pdfPageCount(path: string): Promise<number> {
  return invoke("pdf_page_count", { path });
}

/** shows the file picker for several pdfs at once. */
export async function chooseFiles(): Promise<string[]> {
  const chosen = await showOpenDialog({
    multiple: true,
    filters: [{ name: "PDF documents", extensions: ["pdf"] }],
  });
  if (Array.isArray(chosen)) return chosen;
  return typeof chosen === "string" ? [chosen] : [];
}

/** shows the file picker for a png or jpeg picture. */
export async function choosePicture(): Promise<string | null> {
  const chosen = await showOpenDialog({
    multiple: false,
    filters: [{ name: "Pictures", extensions: ["png", "jpg", "jpeg"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}

/** the last part of a path, for messages. works with either kind of slash. */
export function fileNameOf(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** the name without ".pdf", for naming new files after it. */
export function stemOf(path: string): string {
  return fileNameOf(path).replace(/\.pdf$/i, "");
}

/** the folder part of a path, with the slash kind it already uses. */
export function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return cut > 0 ? path.slice(0, cut) : path;
}

/** a path in the same folder as `beside`, named `name`. */
export function pathBeside(beside: string, name: string): string {
  const separator = beside.includes("\\") ? "\\" : "/";
  return `${folderOf(beside)}${separator}${name}`;
}
