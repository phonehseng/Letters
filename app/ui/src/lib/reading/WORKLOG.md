# reading (long documents) worklog

## 2026-11-12 page finder, history and progress
`jump.ts` mirrors core/search's resolve_jump so Go To answers on every key
without a round trip; both have the same test cases. `history.ts` remembers
jumps only (not every scroll), so Back goes where a reader expects.
`progress.ts` writes "Page 41 of 300 · 13% · About 2 hours left" from the
reader's own measured speed, and leaves the time out until there is one.
Keys: Ctrl G Go To, Alt Left and Right for Back and Forward (Cmd [ and ]
are the writer's indent keys), Ctrl D Add Bookmark as in Preview.
