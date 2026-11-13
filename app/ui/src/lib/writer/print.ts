// printing through the system's print dialog.
//
// a pdf is printed from its pages drawn by the pdf engine at printer
// resolution (300 dpi), one sheet per page, at the page's own size, with no
// margins added. a writer document is first made into a print-ready pdf by
// Word or LibreOffice (exact pages) and printed the same way; without them
// the editing surface itself is printed and the person is told page breaks
// may differ.

import { scaleMilli, tilesForPage, type PageSize } from "../layout";

/** makes a tile's address (api.ts tileUrl); passed in so this file needs no window to test. */
export type TileAddress = (documentId: number, page: number, scale: number, column: number, row: number, revision: number) => string;

export const PRINT_DPI = 300;
/** css pixels per point (96 to the inch, 72 points to the inch) */
const CSS_PER_POINT = 96 / 72;

export interface PrintTile {
  src: string;
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface PrintPage {
  /** css pixels, at the page's real size */
  width: number;
  height: number;
  tiles: PrintTile[];
}

/** where each page and its tiles go on paper, at printer resolution. */
export function printLayout(documentId: number, pages: PageSize[], revision: number, tileUrl: TileAddress, dpi = PRINT_DPI): PrintPage[] {
  // tiles are rendered at `dpi`; on paper the page keeps its size in css pixels
  const devicePerCss = dpi / 96;
  // the engine's scale is device pixels per point, times 1000
  const scale = scaleMilli(1, dpi / 72);
  return pages.map((size, index) => {
    const width = size.width * CSS_PER_POINT;
    const height = size.height * CSS_PER_POINT;
    const tiles = tilesForPage(size, scale, devicePerCss, 0, height).map((tile) => ({
      src: tileUrl(documentId, index, scale, tile.column, tile.row, revision),
      left: tile.left,
      top: tile.top,
      width: tile.width,
      height: tile.height,
    }));
    return { width, height, tiles };
  });
}

/** the @page rule: the first page's size, no printer margins added. */
export function pageRule(first: PageSize | undefined): string {
  if (!first) return "@page { margin: 0 }";
  return `@page { size: ${first.width}pt ${first.height}pt; margin: 0 }`;
}

function waitForImages(root: HTMLElement): Promise<void> {
  const images = [...root.querySelectorAll("img")];
  return Promise.all(images.map((image) => (image.complete ? Promise.resolve() : new Promise<void>((done) => ((image.onload = () => done()), (image.onerror = () => done())))))).then(() => {});
}

/**
 * opens the system's own print dialog for the print view (the shell's
 * writer_system_print, on windows and macs); where that isn't there, as on
 * linux, the web view's printing.
 * returns which one opened.
 */
export async function openPrintDialog(): Promise<"system" | "webview"> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("writer_system_print");
    return "system";
  } catch {
    window.print();
    return "webview";
  }
}

/** how long a print view stays ready after the dialog opened: the system dialog says nothing when it is done. */
const PRINT_VIEW_MINUTES = 3;

/** prints pages the pdf engine draws, through the system's print dialog. */
export async function printPages(documentId: number, pages: PageSize[], revision: number): Promise<"system" | "webview"> {
  const { tileUrl } = await import("../api");
  // one print view at a time
  document.querySelectorAll(".letters-print").forEach((old) => old.remove());
  const holder = document.createElement("div");
  holder.className = "letters-print";
  const style = document.createElement("style");
  style.textContent = `${pageRule(pages[0])}
    @media screen { .letters-print { position: fixed; left: -100000px; top: 0; } }
    @media print { body > *:not(.letters-print) { display: none !important; } body { overflow: visible !important; } }
    .letters-print .sheet { position: relative; overflow: hidden; break-after: page; }
    .letters-print .sheet:last-child { break-after: auto; }
    .letters-print img { position: absolute; display: block; }`;
  holder.append(style);
  for (const page of printLayout(documentId, pages, revision, tileUrl)) {
    const sheet = document.createElement("div");
    sheet.className = "sheet";
    sheet.style.width = `${page.width}px`;
    sheet.style.height = `${page.height}px`;
    for (const tile of page.tiles) {
      const image = document.createElement("img");
      image.src = tile.src;
      Object.assign(image.style, { left: `${tile.left}px`, top: `${tile.top}px`, width: `${tile.width}px`, height: `${tile.height}px` });
      sheet.append(image);
    }
    holder.append(sheet);
  }
  holder.dataset.pages = String(pages.length);
  document.body.append(holder);
  await waitForImages(holder);
  // the print view goes once the dialog is done with it (the web view says
  // so with afterprint; the system dialog doesn't, so a timer removes it)
  window.addEventListener("afterprint", () => holder.remove(), { once: true });
  setTimeout(() => holder.remove(), PRINT_VIEW_MINUTES * 60_000);
  return openPrintDialog();
}

/** prints a pdf file: opened in the engine for its pages, printed, closed. */
export async function printPdfFile(path: string): Promise<"system" | "webview"> {
  const { closeDocument, openDocument } = await import("../api");
  const opened = await openDocument(path);
  const close = () => closeDocument(opened.id).catch(() => {});
  try {
    return await printPages(opened.id, opened.pages, opened.revision);
  } finally {
    window.addEventListener("afterprint", close, { once: true });
    setTimeout(close, PRINT_VIEW_MINUTES * 60_000);
  }
}
