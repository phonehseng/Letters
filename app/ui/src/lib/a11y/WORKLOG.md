# Worklog: the Accessibility tab's logic (app/ui/src/lib/a11y)

## 2026-11-11 pure modules for the checker and wizard

Added `a11yApi.ts` (core/a11y's report and the shell's `FixRequest`, typed),
`findings.ts` (Must Fix / Should Fix / Check by Hand, where a finding is in
words, a key that survives a re-check), `wizard.ts` (steps, Skip, Back,
"3 of 11", refreshing after a re-check so fixed steps close and new ones
join), `ladder.ts`, `languages.ts`, `contrast.ts`, tested in `a11y.test.ts`.
Why the key uses object numbers: lopdf keeps them across edits, so a finding
on the same tag is recognised after a fix or an undo.

## 2026-11-11 the session and the page overlay

Added `a11ySession.svelte.ts` (report, wizard, outlined spot, numbered
reading order; checks again after every fix and after undo while the wizard
is open) and `A11yMarks.svelte`. Why check again in full: the engine has no
way to check only some rules, and a check without the renderer would mark
contrast as not checked, which the wizard would mistake for fixed.

## 2026-11-11 the app's own colours

Added `tokenAudit.ts`: every text and control token pair from app.css in
light, dark and both with increased contrast. It found six pairs under WCAG's
4.5:1 (white on the accent, accent text on a pressed button, red text);
app.css was changed until the test passed. Why a unit test: the audit can't
drift from the tokens the app really uses.

## 2026-11-12 secondary text on controls

axe found the sidebar's unchosen segment under 4.5:1 in the real app. Added
the pair "secondary text on a control on glass" (layers laid over each other
from the bottom) and raised `--label-secondary` to 0.6 / 0.62.

## 2026-11-12 readability pass

Added a README listing the files, and an opening comment to the tests.

## 2026-11-12 proof in the real app

Ran `cdp-compare/a11y.mjs` (22 checks): an untagged handout checked, fixed through the wizard (tags, language, title, contrast, alt text, headings, decorative), checked again clean, undo and redo, saved and reopened. Mark as PDF/UA stayed off because the handout's standard fonts aren't embedded, which is the engine's honest answer.
