# shell-core standards worklog

`app/shell-core/src/standards/`: the parts of Prepare & Check that need no
window.

## 2026-11-12: pdf/a copies, smaller copies, fonts and a renderer

`convert_copy` loads a pdf, runs core/standards' conversion and saves it
with `save_pdfa`; `optimized_copy` runs `optimize`. Neither changes the
source: the shell writes the bytes where the person chose, so the open
document (which may be signed) is never rewritten.

The conversion asks for fonts by postscript-style names ("Arial-BoldMT");
the font library knows families. `font_source` turns the name into its
family ("Arial", "Times New Roman" from "TimesNewRomanPS-ItalicMT") and
then tries the metric-compatible substitutes core/fonts lists, all of which
keep the widths. A family that merely looks alike is not offered: the
conversion couldn't vouch for its widths. `FileRenderer` draws pages of the
source file with the app's engine for pdf/a-1 transparency, laid on white.

Found while testing: a source whose last cross-reference section is a
stream (lopdf saves files that way) keeps that stream object through a
pdf/a-1b conversion, and core/standards' own checker then refuses the copy
under ISO 19005-1 6.1.4. Parts 2 and 3 are unaffected. That is in
core/standards, outside this folder, so the tests here cover 2b, 2u and 3b
and the gap is reported.

## 2026-11-12: signed documents aren't converted

Measured in the app: converting the signed expense claim to PDF/A-2b gave
a copy core/standards' checker passed, but veraPDF failed it under ISO
19005-2 6.4.3: the two signatures' byte ranges no longer covered the
rewritten file. A signature can't survive a rewrite, so `convert_copy`
now refuses a signed source with a sentence saying to convert first and
then sign the PDF/A copy (signing appends, so the copy stays PDF/A). The
checker not testing 6.4.3 is reported for core/standards.
