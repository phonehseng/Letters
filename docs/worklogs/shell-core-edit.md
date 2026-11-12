# Worklog: direct editing helpers (shell-core/src/edit)

What changed in this folder and why, newest last.

## 2026-11-11 helpers around letters-edit for the Edit tab

`fonts.rs` hands the web view a page's embedded font only when it is a
TrueType or OpenType file whose own cmap draws every letter of the text,
because a subset with private codes would show the wrong letters in the
editor. `fit.rs` adds Shrink to Fit: the engine reflows a block to its own
(shrunk) width and has no size-to-fit option, so the words are shared over
the paragraph's original number of lines first and the text then made
smaller to the old width, on a copy, so it stays one undo step and a
failure changes nothing. `pictures.rs` refuses anything but png and jpeg
before the engine sees it, with a sentence pointing to New from Images.
`block_matching` finds a block again by place, since ids change with
every edit.

## 2026-11-12 fairer line sharing in Shrink to Fit

`balance_lines` took words until a line reached its share, which put
three of four words on the first line; it now stops when the next word
would take the line further past its share than it is short of it.

## 2026-11-12 the font library lends its fonts to edits

`DonorCache` makes letters-edit's `FontDonors` from the font library's
index and keeps them until the index changes (a scan finishing, a font
added). Each donor keeps its font bytes once read, so making the pool per
edit would read the same font files again on every keystroke's commit.
