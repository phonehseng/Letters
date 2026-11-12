// where page boundaries go on the editing surface.
//
// this is an estimate, and the docs say so. word and libreoffice place
// page breaks with their own line-breaking, widow rules and font metrics;
// Letters measures how tall each top-level block is on screen and starts
// a new page where the next block would not fit. a block taller than a
// page is not split, so it runs past the bottom of its page. exact page
// breaks come from word or libreoffice when the document is exported.

export interface BlockBox {
  /** height in css pixels at 100% zoom, margins between blocks included */
  height: number;
  /** a page break block, or a paragraph set to start on a new page */
  breakBefore?: boolean;
  /** a page break block itself takes no room on either page */
  isPageBreak?: boolean;
}

export interface Pages {
  /** index of the first block on each page; page 1 starts at 0 */
  starts: number[];
  /** how much of the last page is used, in css pixels */
  lastPageUsed: number;
}

export function paginate(blocks: BlockBox[], pageHeight: number): Pages {
  const starts = [0];
  let used = 0;
  blocks.forEach((block, index) => {
    if (index > 0 && block.breakBefore && used > 0) {
      starts.push(index);
      used = 0;
    }
    if (block.isPageBreak) {
      // the next block starts a new page
      if (index + 1 < blocks.length) starts.push(index + 1);
      used = 0;
      return;
    }
    if (used > 0 && used + block.height > pageHeight) {
      starts.push(index);
      used = 0;
    }
    used += block.height;
  });
  // a page break as the last block still makes an (empty) page
  const last = blocks[blocks.length - 1];
  if (last?.isPageBreak) starts.push(blocks.length);
  return { starts: [...new Set(starts)].sort((a, b) => a - b), lastPageUsed: used };
}

export function pageCount(pages: Pages): number {
  return pages.starts.length;
}

/** "Page 2 of 5" style estimate for the status bar. */
export function pageLabel(current: number, total: number): string {
  return `Page ${current} of about ${total}`;
}

/** the page a block is on, counted from 1. */
export function pageOfBlock(pages: Pages, blockIndex: number): number {
  let page = 1;
  for (let i = 1; i < pages.starts.length; i += 1) if (pages.starts[i] <= blockIndex) page = i + 1;
  return page;
}
