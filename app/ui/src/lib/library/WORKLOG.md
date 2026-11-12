# library (Search All Documents) worklog

## 2026-11-12 the search screen and its rules
`results.ts` holds every rule of the screen without the dom: result rows,
arrow and page keys, highlighted snippet pieces, filter chips and each
sentence (counts, "Indexing 132 of 480 documents", empty states with a next
step). `arrival.ts` hands "open at this page with this query" to the
document's screen through local storage, so it works for a new window too.
The screen is reached with Ctrl Shift G: Ctrl Shift F was the natural key
but already opens Find a Topic, and the rules say not to shadow keys.
`LibrarySettingsPanel` is the settings section, used on the screen and in
the Settings window.

## 2026-11-12 the Find bar gets plain words
The real-app proof showed a hit opening on the right page but the Find bar
finding nothing: it was given "quoted" text, and in-document find matches
plain text. findTextFor drops quotes, OR and -words and uses corrected typos.
