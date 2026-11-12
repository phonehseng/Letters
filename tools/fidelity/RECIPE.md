# PDF to DOCX "exact match" recipe (round 4)

This is the conversion algorithm as the prototype in `prototype/` runs it, written so it can
be ported without reading the Python. Every number and rule about Microsoft Word below was
measured on **Word 16.0 build 16.0.20527 on Windows** (tag: *[measured, 16.0.20527]*).
Nothing here has been checked on another build, another machine, or another default
printer.

Goal: Word's own PDF export of the DOCX puts every glyph within 0.125 pt of the source PDF,
mean under 0.05 pt, with the same embedded fonts.

Units used throughout:

| name | size | where |
|---|---|---|
| pt | 1/72 in | PDF user space |
| twip | 1/20 pt | `w:framePr`, `w:spacing`, `w:pgSz` |
| px | 1/600 in = 0.12 pt | Word's layout grid |
| u | 1/3000 in = 0.024 pt | Word's PDF writer print grid |
| half point | 0.5 pt | `w:sz` |
| EMU | 1/12700 pt | DrawingML |

A twip is 5/12 px. 60 twips = 25 px exactly, so every twip rule below repeats every 60 twips.

---

## 1. Pipeline

1. Read the PDF content streams (section 2).
2. Rebuild every font face as clean TrueType (section 3).
3. Build one grid font per (face, PDF size) (section 4).
4. Group glyphs into lines (section 5).
5. Make sure the model facts for this document are known: writer step tables for every page
   size, baseline tables for every exact line height (section 9). Measure what is missing.
6. Place each horizontal line in a frame, plan every glyph on Word's print lattice, write
   runs (sections 6 and 7).
7. Rotated lines go into rotated text boxes, placed by measuring exports (section 8).
8. Vectors and pictures (section 10).
9. Write the DOCX, export it once through Word, re-plan the few lines Word moved, write it
   again (section 7.6).

---

## 2. Reading the PDF

Interpret the content streams yourself (PDF 1.7, 9.4): pdfium's text API gives neither char
codes nor the program behind a glyph. For every glyph record: page, font, code, Unicode
text, pen origin (from the text rendering matrix `[Tfs*Th 0 0 Tfs 0 Trise] x Tm x CTM`),
size (length of the matrix's vertical axis), horizontal ratio, angle
(`atan2(b, a)`), advance (`(w/1000*Tfs + Tc [+ Tw for code 32 in one-byte fonts]) * Th`
times the matrix's horizontal scale), fill colour, render mode, and its order on the page.
Type 3 widths are in glyph space: multiply them by `FontMatrix[0] * 1000` to get 1/1000 em.

Paths and images come from the same pass, so their order relative to text is kept.

---

## 3. Font rebuilding rules

One output face per PDF face. Subsets of the same face (same base name after the `ABCDEF+`
prefix and the same program kind) are merged into one face. Type 3 fonts without a
`/BaseFont` are named from the descriptor's `/FontFamily` plus their object number.

For each face:

1. **Glyph source.**
   - `FontFile2` and `FontFile3/OpenType`: read as sfnt.
   - `FontFile3/Type1C` and `/CIDFontType0C`: read the bare CFF.
   - `FontFile`: read as Type 1 (eexec).
   - Type 3: each glyph procedure's filled paths (`m l c v y h re` closed by `f F f* B B* b b*`),
     transformed by `cm` and `FontMatrix * 1000` into a 1000-unit em, y up.
     - Report strokes, images and text inside a procedure; they are not rebuilt.
2. **Glyph lookup by code.**
   - CID fonts: CID through `CIDToGIDMap` (identity if absent), or `cidNNNNN` in a CID-keyed CFF.
   - Simple TrueType: the PDF spec's order: (3,0) cmap with code, then `0xF000 + code`, then
     (1,0); then the (3,1) cmap by the code's Unicode; then the glyph name from the encoding.
   - CFF and Type 1: by glyph name from the encoding, then the font's built-in encoding.
3. **Outlines.**
   - Every cubic source (CFF, Type 1, Type 3) is converted to quadratic with a tolerance of
     1 unit per 1000 em (cu2qu, reversing direction).
   - Glyphs are scaled to the face's units per em and composites are decomposed.
   - **Word ignores embedded CFF without warning and falls back to Calibri**
     *[measured, 16.0.20527, E1]*.
4. **Characters.**
   - Each glyph gets the code point of its text: ToUnicode first, then the encoding's glyph
     name through the Adobe Glyph List.
   - A private-use code point from the supplementary area (U+F0000 and up) is used instead
     when the text is:
     - empty, more than one char (a ligature), or a surrogate;
     - already taken by another glyph;
     - a control char below U+0020 (a browser mapped a glyph to U+0000, and a NUL in
       `document.xml` makes Word refuse the file).
   - The report counts private-use glyphs, because their text in the DOCX is not the real
     text.
   - Not U+E000..U+F8FF: Word drew those chars in Calibri even though the embedded font
     maps them. A `w:sym` element did not change that, and a (3,0) symbol cmap made Word
     treat the whole font as a symbol font *[measured, 16.0.20527]*. U+F0000 and up were
     drawn from the embedded font.
   - A space glyph is never a separate glyph: the char is `" "`.
5. **Advances** are the PDF widths: `round(width/1000 * units_per_em)`.
6. **Tables.**
   - Write a fresh TrueType with FontBuilder: glyf, hmtx, a cmap with only the chars above,
     hhea and OS/2.
     - Ascent and descent come from the PDF descriptor (defaults 0.8 / -0.2 em).
     - The family name is unique per document (`Ltr <8 hex>`), so Word cannot find an
       installed font.
   - **OS/2 ulCodePageRange1 bit 0 (Latin 1) is always set**: Word silently replaced a font
     that claims no code page with Times New Roman *[measured, 16.0.20527, round 3]*.
   - fsType is copied from the source program (0 when the source has none).
7. **Embedding permission.**
   - If the source fsType is restricted only (bit 1 set, bits 2 and 3 clear), the face is
     not embedded and the stand-in rule (8) applies; the report says so.
   - fsType 4 (preview and print) is embedded, but Word is expected to open the DOCX
     read-only. That is not verified.
8. **Non-embedded fonts and stand-ins.**
   - Standard 14 names map to metric-compatible open fonts: Helvetica and Arial to Arimo,
     Times to Tinos, Courier to Cousine. Anything else gets Tinos (serif flag) or Arimo.
   - The stand-in's outlines are used with the PDF's widths as advances, so positions stay
     exact while shapes differ.
   - Optional: `LETTERS_STANDINS=windows` uses the Windows fonts instead (`arial*.ttf`,
     `times*.ttf`, `cour*.ttf`, all fsType 8 on this machine). They are not open fonts:
     never ship them, use them only if present on the user's machine.

---

## 4. Grid fonts (one per face and PDF size)

For each (face, PDF size `S`, rounded to 0.01 pt):

1. Stored size `H = round(2*S)/2` (a DOCX stores half points).
2. Units per em `U = 50 * 2H` (so one 1/600 in pixel at size H is exactly 12 units).
3. Drawn size `D = round(H / 0.12) * 0.12`. Word draws glyphs at a whole number of pixels per
   em: 10.5 is drawn at 10.56, 11 at 11.04, 14 at 14.04, 21.5 at 21.48
   *[measured, 16.0.20527]*.
4. Scale outlines and metrics to `round(U * S / D)` units, then relabel the em as `U`. The
   glyphs then draw at size S while layout runs at H.
5. Snap every advance to a whole pixel: `round(advance / 12) * 12` units.
6. Delete GPOS, kern, hdmx, LTSH and VDMX; recompute `hhea.advanceWidthMax`; rename the
   family (`<face family> <S with . replaced by h>`).

Layout advance of glyph g in pixels: `hmtx[g] / 12`.

---

## 5. Lines

- A line is a run of glyphs in content order that share a baseline (within 0.01 pt across
  the baseline) and an angle (within 0.05 degrees), and have the same render mode.
- A glyph that sits more than 1.5 em right of the previous one, or more than half an em
  behind it, starts a new line.
- Superscripts and subscripts change the baseline, so they are lines of their own, placed
  exactly.
- A gap of more than 0.18 em between words with no space glyph gets a virtual space char.
  It is placed at the previous glyph's pen plus its advance.
- Trailing spaces are dropped.
- Exact line height in twips: `20 * max(2, ceil(1.25 * max size in the line))`.

---

## 6. Word's coordinate model *[all measured, 16.0.20527]*

### 6.1 Frame twips to layout pixels (both axes)

```
twips_pixel(t):
    p, r = divmod(5*t + 1, 12)
    if r == 0 and t mod 60 in {19, 55}: p -= 1      # on a pixel edge: down for these two
    return p                                        # (up for t mod 60 in {7, 31, 43})
```
Holds for 85,571 frames (e10, letter, A4 and a 1008 x 612 pt page), x and y, no
exception. Every pixel has a twip value; take the smallest t with `twips_pixel(t) == p`.

### 6.2 Page size

The PDF media box is the DOCX page size rounded to whole pixels:
`W_px = round(W_pt / 0.12)`, `H_px = round(H_pt / 0.12)`.
For example, A4 595.3 x 841.9 pt becomes 595.32 x 841.92.

### 6.3 The PDF writer's print lattice

A glyph that **starts a text object** is placed by its `Tm` alone:
```
x_units = 5 * p_x + k_x(p_x)                 # p_x: layout pixel from the left edge
x_pt    = print(x_units * 0.024)
top_units = 5 * p_top + k_y(p_top)           # p_top: baseline pixel from the top edge
y_pt    = print((5 * H_px - top_units) * 0.024)
k(p)    = number of step pixels s with s <= p     (from the step table of that axis and page size)
print(v)= v rounded to 5 significant digits, at most 3 decimals, trailing zeros dropped
          (72.024, 108.86, 752.4; no ties occur because 0.024*u never ends in 5 at the cut)
```
Measured step tables (first pixel of each extra unit):

| axis, page px | steps |
|---|---|
| x 5100 (letter width) | 506, 1586, 2665, 3743, 4823 |
| y 6600 (letter height) | 654, 2049, 3444, 4839, 6234 |
| x 4961 (A4 width) | 492, 1542, 2591, 3640, 4690 |
| y 7016 (A4 height) | 696, 2180, 3663, 5147, 6630 |
| x 8400 (1008 pt) | 746, 2337, 3927, 5518, 7108 |
| y 5100 (612 pt high) | 494, 1548, 2602, 3655, 4709 |
| x 6000 (720 pt) | 633, 1982, 3331, 4680 |
| y 3375 (405 pt) | 331, 1036, 1741, 2446, 3151 |

About the step tables:

- The steps are not a function of the axis length alone: x 5100 and y 5100 differ.
- They are not exactly evenly spaced (letter x: 1080, 1079, 1078, 1080).
- The fifth step wraps: `5p + 5 = 5(p+1)`. A decoder that assumes k < 5 reads it one
  pixel late.
- Measure a new page size before using it (section 9).

Inverse (`decode`): for every unit value `u` that prints as the number, the pixel `p` with
`5p + k(p) = u` (try `p = u // 5, u // 5 - 1, u // 5 - 2`).

### 6.4 Positions inside a text object (avoid)

- Word merges neighbouring runs with identical formatting into one text object.
- Later glyphs of that object are placed with `Tc` and `TJ` numbers in 1/1000 em at the drawn
  size, against the font's integer `/Widths`.
- Word only corrects once its error passes about 3/1000 em: 0.25 px at 9 pt, 0.5 px at 20 pt,
  0.9 px at 36 pt.
- When the drawn size differs from the stored size (10.5 vs 10.56), the in-object error
  random-walks up to about 0.55 px.
- This is the cause of the round 3 "writer residue" (up to 0.497 px) and of the "large text
  follows the model less well" finding: the layout pixels themselves were right.
- **Rule: every glyph must start its own text object.** Neighbouring runs that differ in any
  property are split (e11). Alternate `<w:lang w:eastAsia="en-US"/>` and
  `<w:lang w:eastAsia="en-GB"/>` from run to run: no effect on Latin text, always a split.

### 6.5 Layout inside a line: advances and `w:spacing` (e15, e16)

```
pen(0) = line start pixel
pen(i+1) = pen(i) + advance_px(i) + spacing_px(s_i)
spacing_px(s) = sign(s) * floor(|s| * 5/12 + 1/2)        # half away from zero, per glyph
```

- Word also keeps the exact sum of the spacings. When that sum and the whole pixels drift
  about a pixel apart, Word moves a glyph by one pixel, at points that could not be
  predicted. This hit 47 of 1,980 steps with random spacings.
- Keep the drift small (section 7.4) and it never happens on the probe: 0 of 1,980 steps.
- In the 13-document corpus, the first export put glyphs off plan in only one line: the
  journal's italic "C. Writer" line (24 glyphs, 7.6).
- The line start (the frame x) is `twips_pixel(frame twips)`. The first glyph's pen is the
  frame's left pixel.

### 6.6 Baselines in exact-height lines

```
line_px = L * 5/12            # L = w:line twips, lineRule exact
if L mod 60 == 40:  baseline = twips_pixel(y + L) - floor(0.2*line_px + 1/2)
else:               baseline = twips_pixel(y) + floor(0.8*line_px + 1/2)
```

- y is the frame's `w:y` in twips; the baseline pixel is counted from the page top.
- Holds for 16 line heights x 60 twip phases, 960 of 960 frames. They were measured with a
  12 pt probe font in the frame; corpus lines with other fonts and sizes landed on it
  (dy = 0 on Word sources).
- The prototype still measures the 60-entry table per line height (section 9) and uses that.

### 6.7 Font size

Layout uses the stored size H; glyphs are drawn at D (section 4).

---

## 7. Placing a horizontal line

### 7.1 Frame XML (one paragraph per line)

```xml
<w:p><w:pPr>
  <w:framePr w:w="{width twips}" w:wrap="through" w:vAnchor="page" w:hAnchor="page" w:x="{x twips}" w:y="{y twips}"/>
  <w:widowControl w:val="0"/><w:suppressAutoHyphens/><w:snapToGrid w:val="0"/>
  <w:autoSpaceDE w:val="0"/><w:autoSpaceDN w:val="0"/><w:adjustRightInd w:val="0"/>
  <w:spacing w:before="0" w:after="0" w:line="{L}" w:lineRule="exact"/>
  <w:ind w:left="0" w:right="0" w:firstLine="0"/>
</w:pPr>{runs}</w:p>
```
Width: `min(line width + 40, max(page width - start x, line width + 4))` pt, in twips.

### 7.2 x: the line start

- `p0 = nearest_exported_pixel(first glyph x)`: the pixel whose printed x (6.3) is closest
  to the source x. Search round(x/0.12) ± 2.
- `x twips = smallest t with twips_pixel(t) == p0`.

### 7.3 y: the baseline

- `guess = round((baseline_from_top - 0.804 * L/20) * 20)`.
- Over `t in guess-12 .. guess+12`, pick the t whose printed y (6.3, with the baseline of 6.6
  and the step table for the page height) is closest to the source's PDF y. Ties go to the
  t nearest the guess.

### 7.4 Glyph targets and spacing

For glyph i (including virtual spaces, at their computed positions):

- `target_i = nearest_exported_pixel(source x_i) - p0`, then force `target_0 = 0`.
- `step_i = target_{i+1} - target_i - advance_px(i)`.
- **Choose `s_i` (twips) among the values with `|s * 5/12 - step_i| < 1/2`** (two or three of
  them). Take the one that keeps the carried remainder `C + s*5/12 - step_i` closest to zero,
  then the smaller |s|. Start with C = 0 and update C after each glyph.
- The remainder then stays within 5/24 px and Word never re-syncs. The last glyph's spacing
  is 0.

### 7.5 Run XML (one run per char)

```xml
<w:r><w:rPr>
  <w:rFonts w:ascii="{grid family}" w:hAnsi="{grid family}" w:cs="{grid family}" w:eastAsia="{grid family}"/>
  [<w:vanish/>]                       <!-- render mode 3 or 7: invisible text -->
  [<w:color w:val="RRGGBB"/>]          <!-- omitted for black -->
  [<w:spacing w:val="{s_i}"/>]         <!-- omitted when 0 -->
  <w:sz w:val="{2H}"/><w:szCs w:val="{2H}"/>
  <w:lang w:eastAsia="{en-US | en-GB alternating}"/>
  <w14:ligatures w14:val="none"/>
</w:rPr><w:t xml:space="preserve">{char}</w:t></w:r>
```

- A space glyph takes the grid font of the glyph before it, or the first non-space glyph at
  the line start.
- No `w:kern`: its presence switches kerning on for sizes at or above its value, so
  `w:val="0"` would kern everything.

### 7.6 Checking export (one or two passes)

1. Export the DOCX through Word.
2. Decode every glyph's layout pixel from its `Tm` (6.3).
3. Compare it with `target_i`. For each line with a wrong glyph:
   - take the first wrong glyph i and the pixels d it is off;
   - subtract d from targets i..n;
   - re-plan the spacing.
4. Rewrite and export again; at most 2 re-plans.
5. Each line keeps the spacing of the pass where it had the fewest misses.

In the corpus one line needed this: the journal's italic "C. Writer" line, where Word laid
out a "W" 1 px wider than its grid advance and moved it back 23 glyphs later.

---

## 8. Rotated lines (DrawingML text boxes)

Frames cannot rotate. A rotated line becomes an anchored text box:

```xml
<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing>
<wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{251658240+id}" behindDoc="0"
  locked="1" layoutInCell="0" allowOverlap="1"><wp:simplePos x="0" y="0"/>
  <wp:positionH relativeFrom="page"><wp:posOffset>{left EMU}</wp:posOffset></wp:positionH>
  <wp:positionV relativeFrom="page"><wp:posOffset>{top EMU}</wp:posOffset></wp:positionV>
  <wp:extent cx="{w EMU}" cy="{h EMU}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/>
  <wp:docPr id="{id}" name="Item {id}"/><wp:cNvGraphicFramePr/>
  <a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
  <wps:wsp><wps:cNvSpPr txBox="1"/>
    <wps:spPr><a:xfrm rot="{round(-angle*60000) mod 21600000}"><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/></a:xfrm>
      <a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln><a:noFill/></a:ln></wps:spPr>
    <wps:txbx><w:txbxContent>{one paragraph: pPr as 7.1 without framePr, runs as 7.5}</w:txbxContent></wps:txbx>
    <wps:bodyPr rot="0" vert="horz" wrap="square" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t" upright="0"><a:noAutofit/></wps:bodyPr>
  </wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice></mc:AlternateContent></w:r>
```

Geometry:

- Box width: the line width plus 20 pt. Box height: L/20 pt.
- Baseline 0.804·L/20 pt below the box top.
- Word turns shapes clockwise about the box centre; PDF angles are counter-clockwise, so
  `rot = -angle`.
- The centre is chosen so the first pen, rotated, lands on the source origin.

Use `wrap="square"`. With `wrap="none"` Word moved the line's start whenever its content got
wider or narrower (19.8 pt in round 3, and it moved again after every re-plan)
*[measured, 16.0.20527]*.

Placement by measuring exports (not cached):

1. Export a DOCX holding only the rotated boxes. Move each box by (target - produced first
   pen).
2. Repeat while the start is off by more than 0.06 pt, at most 3 more moves.
   - The produced start moved in steps of about 0.36 pt (3 px) when the box moved
     (-0.184, +0.176, -0.184, +0.104 pt on the slides), so stop at the best.
3. One more export measures where each line now starts along its baseline (`o`).
   - Plan the glyphs against that start: `target_i = round((along_i - o) / 0.12)`, with
     `target_0 = 0`, and spacing as in 7.4.
   - The print lattice of rotated text objects was not modelled: the along positions sit on a
     0.12 pt lattice, but its step table is unknown.

---

## 9. Calibration: what is measured, and the cache

Only two kinds of facts are measured, once per Word build. They are cached in
`out/calibration_cache/word-<build>-model.json`:

```json
{"steps":    {"x:<W_px>": [step pixels], "y:<H_px>": [step pixels]},
 "baseline": {"<L twips>": [60 offsets, index = y twips mod 60]}}
```

Printed point values never go into the cache: they depend on the page size. That is why
round 3's cache keyed on (line height, twips) disagreed with itself on 0.9% of y values: its
probe pages were not the size of the document pages.

**Step table probe for one axis and page size** (a probe document of that page size, 12 pt
probe font, `n` in a 200-twip-wide frame, 240-twip exact lines):

- x pass:
  - Coarse: one frame per row, frame x at `smallest t with twips_pixel(t) = p` for
    p = 0, 5, 10, ... up to `W_px - 100`.
  - Read each Tm x and take the unit value nearest `5p`; `k = u - 5p`.
  - Fine: every pixel between two coarse pixels whose k differ.
- y pass: the same, with frames in 16 columns 700 twips apart, frame y set so the baseline
  sits on p (baseline offset 80 px for L = 240, constant), p from 81 to `H_px - 60`.
  - A column's next frame is at least 120 px lower; otherwise a new page starts.
- The steps are the first pixels at which k reaches 1, 2, ....

**Baseline probe for one line height L** (letter page, letter y steps known):

- 60 frames, frame y = `1200 + r + 60*rows_apart*(r // 16)` for r = 0..59, in 16 columns.
- `rows_apart = 1 + (L+200)//60`.
- `offset[r] = decode_y(Tm y) - twips_pixel(y)`.

**Verify**: re-measuring all 19 facts the base corpus uses gave 0 disagreements; 4 more
facts for the round 4 additions also gave 0.

---

## 10. Vectors and pictures

Per page, one 1 pt anchor paragraph (`w:line="20" exact`, `w:sz 2`) carries every floating
drawing of the page in content order, all `behindDoc="1"`.

- **Simple paths become DrawingML shapes.** Simple means:
  - axis-aligned rectangles (an `re`, or 4 points closed), filled and/or stroked;
  - straight stroked segments;
  - solid colour, no dash, alpha 1, no pattern;
  - every clip a single rectangle that contains the path within 0.5 pt.

  Rectangles use `prst="rect"` with `a:solidFill` and/or `a:ln w=EMU`. Segments use
  `prst="line"` with flipV when the slope runs the other way.
- **Everything else is rasterised**: curves, dashes, transparency, shadings, patterns, real
  clips, filled polygons.
  - Copy the page's resources into a new one-page PDF that holds only those items' operators.
  - Render it at 300 dpi with a transparent background and crop to their bounding box
    (+1 pt).
  - Place it as a PNG picture at that box.
  - Text is never rasterised.
- **Pictures.**
  - A JPEG with no mask in DeviceRGB or DeviceGray keeps its original bytes, including
    reportlab's ASCII85-wrapped ones; everything else is re-encoded as PNG with its SMask
    as alpha.
  - Placed with `pic:pic`, the extent from the image matrix, `flipH`/`flipV` for negative
    scales, `rot` for rotated matrices.
- **Sections and pages.** One section per page size: `w:pgSz` in twips, landscape when wider
  than tall, all margins 0, `w:cols w:space="0"`. Pages of the same size are separated by a
  1 pt paragraph with `w:pageBreakBefore`.
- **Fonts.** Embedded fonts are obfuscated `.odttf` parts (ECMA-376 17.8.1: XOR the first
  32 bytes with the GUID key read backwards). The `w:font` entries point at them with
  `w:embedRegular`.
- **settings.xml:** `w:embedTrueTypeFonts`, `compatibilityMode` 15,
  `characterSpacingControl doNotCompress`.

---

## 11. Known limits

- **Grid floor.** Positions can only be lattice points: 0.12 pt apart (0.144 pt across a
  step). On sources off the lattice the best possible is max 0.072 pt and mean about
  0.03 pt per axis. Each corpus class's floor is in `corpus_report.md`. All classes sit on
  it within 0.002 pt on the mean.
- **Two axes.** A reportlab or browser source misses both x and y, so the combined mean
  floor is about 0.045 pt. The journal page's floor is 0.0497 pt, just under the 0.05 pt
  target, because its 11.4 pt leading puts most baselines far from the lattice
  (y mean 0.035).
- **Rotated text.**
  - Positions along the line are planned on a measured start, but the rotated print
    lattice is not modelled.
  - The box moves in coarse steps, so a rotated line's start can stay up to about 0.06 pt
    off.
  - Each document with rotated text costs 4 to 6 extra exports.
- **Fonts.**
  - Non-embedded fonts can only get stand-ins.
  - A restricted fsType forces a stand-in.
  - Type 3 glyphs keep only filled paths.
- **Editing.**
  - One frame per line, one run per char, alternating East Asian language tags, and
    font sizes rounded to half points (glyphs scaled).
  - Phrase search across runs works; reflow after editing was not tested.
- **Unverified.**
  - Other Word builds, another default printer, other page sizes until measured, and
    screen versus PDF.

## 12. Word behaviours this recipe depends on

All *[measured, 16.0.20527, Windows, default printer "Microsoft Print to PDF" 600 dpi]*:

1. Layout positions are whole 1/600 in pixels, inside a line and for frames.
2. Frame twips map to pixels by `twips_pixel` (6.1), including the 60-twip edge pattern.
3. The PDF media box is the page size rounded to whole pixels.
4. The writer prints Tm on a 1/3000 in lattice with a per-page-size step table (6.3), 5
   significant digits, at most 3 decimals.
5. Inside one text object positions come from Tc/TJ with a ~3/1000 em correction threshold,
   so they are not on that lattice.
6. Runs that differ in any property, including `w:lang w:eastAsia`, become separate text
   objects.
7. Each glyph's `w:spacing` becomes whole pixels on its own, rounded half away from zero.
   Word re-syncs to the exact sum when the two drift about a pixel apart.
8. Exact line height puts the baseline by the rule in 6.6.
9. Stored sizes are half points; glyphs are drawn at whole pixels per em; layout uses the
   stored size.
10. Embedded TrueType (whole or subset) is used, with or without `w:embedTrueTypeFonts`.
    Embedded CFF is ignored and replaced by Calibri.
11. A font that claims no OS/2 code page is replaced by Times New Roman.
12. `w:kern` switches kerning on at and above its value.
13. A frame paragraph with `w:wrap="through"` keeps its position regardless of body flow.
14. A DrawingML text box with `wrap="none"` shifts its line when the content width changes;
    `wrap="square"` does not.
15. "Spacing before" is dropped on the first paragraph after a page break.
16. Word rejects a DOCX whose `w:t` holds U+0000.
17. Word draws text in U+E000..U+F8FF with Calibri instead of the run's embedded font; text in
    U+F0000.. comes from the embedded font.
