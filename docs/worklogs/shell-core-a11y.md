# Worklog: the accessibility checker in the shell (app/shell-core/src/a11y)

## 2026-11-11 the region renderer, fixes and blocks

Added `region.rs` (the `RegionRenderer` over the worker: the page is drawn
once and areas cut from it, because the checker asks for hundreds of areas
per page), `fixes.rs` (`FixRequest` as the window sends it and `apply_fix`,
so every fix is one call inside one `state.edit`) and `blocks.rs` (where each
tagged block of a page is). Why blocks.rs exists: `auto_reading_order`
returns tags without places, and the wizard must number them on the page.
Text widths there are estimated from letter counts; the fonts are not read.

## 2026-11-11 tests

`tests.rs`: the request shapes, each fix through an edit session as one undo
step, an edited heading ladder, contrast measured from pdfium's pixels with
the page drawn once, regions cut at the page's edge, and reading blocks in
order with their places and words.

## 2026-11-12 readability pass

Split `marked_content` (121 lines) into a `Walker` with one method per kind of drawing instruction, and named the letter-width, ascent and descent estimates, as CONTRIBUTING.md asks. Behaviour is unchanged.
