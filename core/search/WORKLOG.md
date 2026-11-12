# core/search worklog

## 2026-11-12 an inverted index of our own
Wrote the index here instead of using tantivy: the app needs a few hundred
lines of it, it builds on the gnu toolchain with no c parts, and it adds
almost nothing to the program. Words are folded (case, accents) and stored
as posting lists of passages (a pdf page or a word paragraph) with varint
gaps, so a 5,000 document library fits in about 50 MB of memory. Passage
text stays in an append-only file and is read only for the results shown.
Checkpoints are written whole and renamed into place, so a crash loses at
most the documents since the last checkpoint, which are simply read again.

## 2026-11-12 queries, ranking and snippets
Plain words must all match; "phrases", -exclude, OR, the last word as a
prefix while typing, english stemming (porter 1980), and a typo match when
a word is not found at all. Ranking is bm25 plus a bonus for title and
heading matches, words close together and recently changed documents.
Excluded words leave out whole documents, because "-plant" means "not about
plants", not "not this page".

## 2026-11-12 page finder and measurements
`resolve_jump` reads "p 12", "xii", "50%", "ch 3", "#methods", "last", "+5"
and "w 5000". Measured on a generated 5,000 document library (21 million
words, release build): indexing 19 s, 207 MB on disk, about 51 MB in memory,
query p50 20 ms and p95 32 ms.

## 2026-11-12 moved files keep their entry
`rename_document` re-points an entry when Letters moves or renames a file,
so its text is not read again.

## 2026-11-12 search.rs split by job
The readability check flagged search.rs at 869 lines. Result types moved to`nresults.rs, word widening to expand.rs and snippets to snippet.rs, each
with one job; behaviour and tests are unchanged.
