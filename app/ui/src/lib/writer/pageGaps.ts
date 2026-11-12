// the space between two pages on the editing surface: the rest of the
// page above, its footer, a strip of desk, and the next page's header.
// headers and footers here are drawn from the document (page numbers
// filled in); the editable ones are on the first and last page.

import { DOMSerializer } from "prosemirror-model";
import { blocksToDoc } from "./convert";
import type { DocumentMeta, HeaderFooterRefs, Section } from "./model";
import type { Pages } from "./pagination";
import type { PageGap } from "./plugins";
import { schema } from "./schema";
import { twipsToPx } from "./units";

/** the gap between pages on the desk, in css pixels at 100%. */
export const DESK_GAP = 28;

/** which header or footer id a page uses. */
export function partFor(section: Section, refs: HeaderFooterRefs | undefined, pageNumber: number, evenAndOdd: boolean): string | undefined {
  if (!refs) return undefined;
  if (pageNumber === 1 && section.differentFirstPage) return refs.first;
  if (evenAndOdd && pageNumber % 2 === 0) return refs.even ?? refs.default;
  return refs.default;
}

/** a header or footer as plain elements, its page number fields filled in. */
export function staticPart(meta: DocumentMeta, id: string | undefined, pageNumber: number, pageCount: number): HTMLElement {
  const holder = document.createElement("div");
  holder.className = "w-doc w-static-part";
  const part = id ? (meta.headersFooters ?? []).find((item) => item.id === id) : undefined;
  if (!part) return holder;
  const fragment = DOMSerializer.fromSchema(schema).serializeFragment(blocksToDoc(part.blocks).content);
  holder.append(fragment);
  holder.querySelectorAll<HTMLElement>(".w-field[data-kind=pageNumber]").forEach((field) => (field.textContent = String(pageNumber)));
  holder.querySelectorAll<HTMLElement>(".w-field[data-kind=pageCount]").forEach((field) => (field.textContent = String(pageCount)));
  return holder;
}

/** the gaps to show for a pagination, one before each page after the first. */
export function pageGaps(meta: DocumentMeta, pages: Pages, used: number[], textHeight: number): PageGap[] {
  const section = meta.sections?.[0];
  if (!section) return [];
  const margins = section.page.margins;
  const top = twipsToPx(margins.top);
  const bottom = twipsToPx(margins.bottom);
  const evenAndOdd = !!meta.settings?.evenAndOddHeaders;
  const total = pages.starts.length;
  return pages.starts.slice(1).map((blockIndex, index) => {
    const pageAbove = index + 1;
    const rest = Math.max(0, textHeight - (used[index] ?? 0));
    const height = rest + bottom + DESK_GAP + top;
    return {
      blockIndex,
      height,
      pageNumber: pageAbove + 1,
      render: () => gapElement(meta, section, { rest, top, bottom, pageAbove, total, evenAndOdd }),
    };
  });
}

function gapElement(
  meta: DocumentMeta,
  section: Section,
  sizes: { rest: number; top: number; bottom: number; pageAbove: number; total: number; evenAndOdd: boolean },
): HTMLElement {
  const margins = section.page.margins;
  const gap = document.createElement("div");
  gap.className = "w-page-gap";
  gap.contentEditable = "false";
  gap.setAttribute("aria-hidden", "true");
  gap.style.height = `${sizes.rest + sizes.bottom + DESK_GAP + sizes.top}px`;
  gap.style.setProperty("--w-margin-left", `${twipsToPx(margins.left)}px`);
  gap.style.setProperty("--w-margin-right", `${twipsToPx(margins.right)}px`);

  const footer = staticPart(meta, partFor(section, section.footers, sizes.pageAbove, sizes.evenAndOdd), sizes.pageAbove, sizes.total);
  footer.classList.add("w-gap-footer");
  footer.style.bottom = `${DESK_GAP + sizes.top + twipsToPx(margins.footer ?? 720) - 4}px`;

  const desk = document.createElement("div");
  desk.className = "w-gap-desk";
  desk.style.bottom = `${sizes.top}px`;
  desk.style.height = `${DESK_GAP}px`;
  const label = document.createElement("span");
  label.textContent = `Page ${sizes.pageAbove + 1}`;
  desk.append(label);

  const header = staticPart(meta, partFor(section, section.headers, sizes.pageAbove + 1, sizes.evenAndOdd), sizes.pageAbove + 1, sizes.total);
  header.classList.add("w-gap-header");
  header.style.top = `${sizes.rest + sizes.bottom + DESK_GAP + twipsToPx(margins.header ?? 720)}px`;

  gap.append(footer, desk, header);
  return gap;
}
