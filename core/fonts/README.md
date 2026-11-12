# letters-fonts

The font library: it finds every font Letters can use, describes each one
in plain words, searches them forgivingly, draws previews with its own
code, picks stand-ins for fonts a document asks for, prepares fonts for
embedding, and downloads free fonts from an online library when the person
asks.

## Where fonts come from

| Source | What it is | Where it lives | Shown as |
|---|---|---|---|
| System | fonts installed on the computer, used where they are and never copied | Windows: `%WINDIR%\Fonts` and `%LOCALAPPDATA%\Microsoft\Windows\Fonts`. macOS: `/System/Library/Fonts`, `/Library/Fonts`, `~/Library/Fonts`. Linux: `/usr/share/fonts`, `/usr/local/share/fonts`, `~/.fonts`, `~/.local/share/fonts` and the `<dir>` entries of `/etc/fonts/fonts.conf` | On This Computer |
| Custom | `.ttf`, `.otf`, `.ttc`, `.woff`, `.woff2` files or folders the person adds; checked, decoded to a plain font and copied | `<app data>/fonts/added` | Added by Me |
| Bundled | a starter set shipped with the app (below) | the app's `fonts` resource folder; `vendor/fonts` while developing | Comes with Letters |
| Library | families downloaded from the online library | `<app data>/fonts/library/<family>` | Downloaded |
| Downloadable | the online library's families not downloaded yet: 2,042 families from the [google/fonts](https://github.com/google/fonts) repository at commit `c30b221beb0c1da59d563feb91ec8656a1fa972a` | `data/catalogue.z` inside the crate (257 kB packed, 789 kB of text) | To Download |

All sources are merged into one index of families (`index.rs`). A family
found in several places is one family; the most local copy of a face wins.

### Scanning

System fonts are scanned on a background thread when the app starts
(`FontLibrary::start_scan`). The index fills in batches of 40 files, so the
ui can show fonts while the scan goes on, with progress. Each file's facts
are cached in `<app cache>/fonts/system-fonts.json`, keyed by path, size
and modification time; a changed file is read again, a removed one is
forgotten, and files that turned out not to be fonts are cached too so a
broken file isn't read at every start. Measured on Windows 11 with 388 font
files (411 faces): 0.85 s for the first scan in a release build, 0.09 s
from the cache.

## What is known about each face (`facts.rs`, `coverage.rs`)

Family, style name, full and PostScript names, weight (100 to 900), width
(1 to 9), italic or oblique, monospaced, variation axes, every character
it maps (as ranges, so "can it draw this?" needs no second look at the
file), the writing systems and languages it supports (a set of telling
letters per system, or the online library's subsets for fonts not
downloaded), its kind (serif, sans serif, monospaced, display,
handwriting, symbols: from the catalogue, or guessed from panose numbers,
the IBM class and the name), its embedding permission from the OS/2
`fsType` bits with a plain sentence, version, glyph count, outline type,
and measurements for finding look-alikes (advance widths of printable
ascii, x-height, cap height, panose, and stroke weight and contrast
measured on the letter "o").

The embedding sentences:

- "This font can be embedded in PDFs and documents." (installable or editable)
- "This font can be embedded in PDFs and documents for reading and printing, but others may not be able to edit text in it." (preview and print)
- "This font’s license doesn’t allow embedding; others will see a similar font unless they have it." (restricted, or bitmap only)
- "… The whole font is embedded, which makes files larger." is added when subsetting is barred.

## Searching (`search.rs`, `fuzzy.rs`)

`search(index, query, personal)` is a pure function. Each typed word must
match the family: by name (whole name, start of the name, start of a word,
anywhere, or with one typo for 4 to 6 letters and two for longer words:
"helv" finds Helvetica, "garmond" EB Garamond), by kind ("mono",
"serif"), by writing system ("greek"), or by designer. Phrases become
filters: "like Arial" or "similar to Arial", "supports Vietnamese", and
"draw: ĐỒNG ₫" (only fonts that can draw every character). The chips add
kind, writing system, place (on this computer, added by me, Letters
library, to download), monospaced, has bold, has italic, variable, number
of styles, embeddable and favourites. Without typed words, favourites
come first, then fonts on this computer, then the online library.

"Similar to" (`similar.rs`) ranks metric-compatible fonts first (every
printable ascii character within 4 thousandths of the em, 1 on average),
then by look. Measured on Windows 11: Arimo matches Arial, Carlito
matches Calibri and Tinos matches Times New Roman exactly (0 difference).
Caladea, made to match Cambria, does not in the version the library has
(2.5% of the em on average, the figures most), so it is offered as similar.

## Previews (`outline.rs`, `raster.rs`, `preview.rs`)

Sample text is laid out with the font's advances and old-style kerning
(no shaping, so complex scripts preview roughly) and returned as svg path
data, or filled into an antialiased alpha bitmap by a small signed-area
scanline rasteriser. Nothing is installed or registered with the system.
A family not downloaded can be previewed after fetching only its regular
file into the cache (`FontLibrary::fetch_preview`).

## Using fonts in documents (`substitute.rs`, `subset.rs`)

- `resolve(family, weight, italic)` gives the closest face, or a
  substitute with a sentence: first the substitution table (metric
  twins: Calibri to Carlito, Arial and Helvetica to Arimo, Times New Roman
  to Tinos, Courier New to Cousine, Georgia to Gelasio; look-alikes for
  about twenty more), else a common sans font that is here.
- `fallback_for(family, char)` follows a fallback chain per writing
  system (Noto first, then the usual system fonts) for characters the
  chosen font can't draw.
- `missing_fonts(names)` is the Missing Fonts view's data.
- `embedding_data(font, index, text)` checks `fsType`, then returns a
  TrueType subset that keeps every glyph number (so a pdf can use
  `CIDToGIDMap /Identity`), or the whole font when the licence bars
  subsetting or the outlines are CFF. The subsetter began as core/edit's
  (`fonts/truetype.rs`); core/edit and the docx writer should move to this
  crate's `embedding_data` later rather than keep their own.
- The web view gets an added, bundled or downloaded font's bytes through
  the `fonts_face_bytes` command and registers them with the FontFace api
  for that window only (`app/ui/src/lib/fonts/fontFaces.ts`).

## Fonts are untrusted input

Files are read with a size limit (64 MB for added fonts, 128 MB for system
fonts) before anything else; the format comes from the first bytes, never
the name; WOFF (zlib) and WOFF2 (brotli plus the glyf, loca and hmtx
transforms) are decoded by bounds-checked code here, with every claimed
size capped; ttf-parser, which never panics on bad input, reads the rest;
every face must parse before an added font is copied. Nothing in a font
is ever run (hinting instructions are copied, never executed). A broken
file is refused with a sentence such as "“Cut.ttf” is damaged and can’t be
used. Download the font again from where you got it." The tests feed
truncated, flipped and random bytes through every reader. ttf-parser has a
few debug-only assertions that hostile files can trip, so the workspace
turns them off for debug builds too (they don't exist in release builds).

WOFF2 collections (several fonts in one `.woff2`) are refused; add the
`.ttc` instead.

## What leaves the computer, and when

Nothing, until the person chooses Download or Show Preview on a font that
isn't on the computer. Then, after a one-time notice, Letters sends one
https request per file to `raw.githubusercontent.com` for that file at the
pinned commit, and nothing else: no account, no key, no identifier, no
telemetry. Each downloaded file's size and git blob hash are checked
against the catalogue before it is used; a mismatch deletes it. Downloads
show progress, can be cancelled, and continue where they stopped. Search,
previews of installed fonts and everything else work offline.

## Licences of everything offered

Every font Letters ships or offers may be redistributed and embedded:

- **Bundled** (fetched by `tools/fetch-fonts.ps1` or `.sh` into
  `vendor/fonts`, pinned by commit and checked by SHA-256, each with its
  licence text): Carlito, Caladea, Arimo, Tinos, Cousine (the
  metric-compatible set), Inter, Source Serif 4, Source Sans 3, Noto Sans,
  Noto Serif, EB Garamond, JetBrains Mono, Caveat (handwriting) and
  Atkinson Hyperlegible (made for low-vision readers). All are under the
  **SIL Open Font License 1.1**, checked in each family's own upstream
  repository (googlefonts/carlito, googlefonts/caladea, googlefonts/arimo,
  googlefonts/tinos, googlefonts/cousine, rsms/inter,
  adobe-fonts/source-serif, adobe-fonts/source-sans,
  notofonts/latin-greek-cyrillic, octaviopardo/EBGaramond12,
  JetBrains/JetBrainsMono, googlefonts/caveat,
  googlefonts/atkinson-hyperlegible). Tinos has no licence file in
  google/fonts at the pinned commit, so its OFL.txt comes from
  googlefonts/tinos at commit `4106aab86ddefe26d58a69745e04ae9479d22993`.
  About 18 MB in all.
- **Online library**: every family of google/fonts' `ofl/`, `apache/` and
  `ufl/` folders, under the SIL Open Font License 1.1, the Apache License
  2.0 or the Ubuntu Font Licence 1.0. A test checks the catalogue holds no
  other licence. Each family's licence file is downloaded with it, and the
  detail pane names the licence.
- **System fonts** are used where they are installed, under whatever
  licence the system has for them. Letters never copies them into the app,
  its downloads or its bundle, and embeds them only as their fsType allows.
- **Test fixtures** (`tests/fixtures`): Atkinson Hyperlegible Regular and
  Bold from google/fonts, and its latin subset as WOFF and WOFF2 from
  Fontsource (`@fontsource/atkinson-hyperlegible` 5.3.0), all SIL Open
  Font License 1.1 (`tests/fixtures/OFL.txt`).

## Making the catalogue again

```powershell
cargo run -p letters-fonts --example build_catalogue -- <commit> core/fonts/data/catalogue.z
```

It asks github once for the commit's file list (names, sizes and git blob
hashes) and reads every family's `METADATA.pb` from
raw.githubusercontent.com. No font file is downloaded. Change the commit in
`tools/fetch-fonts.*` at the same time so the bundled set stays the same
as the library's copy.

## Libraries

ttf-parser (MIT or Apache-2.0), miniz_oxide (MIT, Zlib or Apache-2.0),
brotli-decompressor (BSD-3-Clause or MIT), sha1 (MIT or Apache-2.0), ureq
with rustls (MIT or Apache-2.0), serde and thiserror (MIT or Apache-2.0).
None restricts distribution.

## Limits

- No text shaping: previews of Arabic, Indic and other complex scripts are
  roughly drawn (letters unjoined). The editor itself uses the web view,
  which shapes properly.
- Variable fonts are previewed and embedded at their default instance;
  previews can set a weight. Embedding another instance would need the
  variations applied to the outlines, which isn't done.
- CFF (`.otf`) fonts are embedded whole, not subset.
- "Can it draw" for fonts not downloaded is judged from the library's
  subsets, so it says "probably".
- The bundled set and catalogue are pinned to one commit; newer upstream
  fixes arrive only when both are rebuilt.
