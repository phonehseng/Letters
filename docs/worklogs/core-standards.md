# letters-standards worklog

Newest entry last. Each entry says what changed and why.

## 2026-11-12 crate planned

The crate will hold PDF/A checks and conversion, PDF/X pre-flight checks,
a general "Check This PDF" and an optimiser. Work on it starts after the
forms and signing work in letters-forms, in the order the brief gives.

## 2026-11-12 pdf/a checks and conversion

Added `src/pdfa/`: `check_pdfa` (objects) and `check_pdfa_file` (also the
bytes) for pdf/a-1b, 2b, 2u and 3b, one function per area: the file's
header, end and stream keywords (`rules_file.rs`), the document's metadata,
output intents, forms, layers and attachments (`rules_document.rs`),
actions, compression and size limits on every object (`rules_objects.rs`),
pages and annotations (`rules_pages.rs`), and what pages draw: fonts,
device colour, part 1 transparency and image flags (`rules_drawing.rs`,
walking every drawing through `walk.rs`). Each finding has a sentence, the
iso 19005 clause, where it is and whether Letters can fix it.
`convert_to_pdfa` fixes what it can and checks again: forbidden features
are removed with a sentence each, xmp with pdfaid is written and the info
dictionary made to agree, an srgb output intent is added from a profile
generated in `icc.rs` (no third-party profile, so no licence to track),
fonts are embedded through a caller's `FontSource` with letters-fonts
checking the licence and subsetting, form fields are drawn by
letters-forms and simple annotations here, and pdf/a-1 transparency is
turned into pictures through a caller's `PageRenderer` (an area for a soft
masked picture, else the whole page with its text kept invisible on top).
`save_pdfa` writes a classic cross-reference table and a binary header
comment. Tests break each rule on a clean file, convert, and run qpdf
--check (through pikepdf) on every output; the stream keyword check was
narrowed after it mistook the word inside /application#2Foctet-stream for a
stream.
## 2026-11-12 veraPDF as an independent check

veraPDF 1.30.4 (the reference pdf/a validator, gplv3+ / mplv2+) was
installed per user in %LOCALAPPDATA%\Programs\verapdf from the project's
own download, its gpg signature checked, to check our conversions with a
tool we didn't write. It is a development tool only: never bundled or
linked. `tests/verapdf.rs` runs it on every conversion at every level
(ignored by default). It found two gaps in our checker, both fixed: pdf/a-1
forbids an /EF entry in every file specification, even one nothing lists
any more (now checked on every object, and `convert_to_pdfa` and
`save_pdfa` prune unused objects), and pdf/a-3 asks every embedded file to
be associated with the document through /AF (now checked, and added by the
conversion).
## 2026-11-12 check this pdf, optimise, pdf/x

Added `preflight` (cross-reference offsets checked against each object's
header, references to missing objects, fonts, each picture's resolution as
placed from `images.rs`, page boxes, what the bytes go to, and a checklist
of features viewers or printers show differently: unembedded fonts, low
resolution, xfa, NeedAppearances, scripts, layers, overprint, blend modes,
media, type 3 fonts, jbig2/jpeg 2000, spot colours). `optimize` drops
unused objects, stores identical streams once, compresses plain streams,
optionally averages pictures down to a chosen resolution, and packs object
streams only when that is smaller; bytes saved are measured per step by
saving after it, and font subsetting, linearisation and jpeg recompression
are listed as not done. `check_pdfx` reports pdf/x-1a and pdf/x-4
problems (output intent, version, trapping, trim and bleed boxes, fonts,
picture resolution, rgb and transparency) and lists spot colours; it
converts nothing, since that needs the press's colour profile. Tests check
a broken table is found and that qpdf finds it too, and render before and
after optimising.
## 2026-11-12 readme

The README says what each check and the conversion cover, by clause, how
they are tested, how veraPDF was installed and is run (a development tool
only, never shipped), how the owner can run it on any file, and every
limit, so no conformance claim rests on our checker alone.

## 2026-11-12 a test race in the rasterising tests

The test support folder for a render was named by the process id alone, so
tests running on parallel threads under the same name ("rasterise") could
wipe or overwrite each other's render.pdf before pdfium read it. On macOS
CI this showed as part_one_transparency_becomes_pictures_and_says_so finding
half its picture changed. Each call now gets its own folder. The converter
itself was not at fault.
