# ui forms worklog

`app/ui/src/lib/forms/` and the Fill & Sign tab's sections in
`app/ui/src/lib/inspector/fillsign/`.

## 2026-11-12: filling runs the form's logic, and Prepare Form sets it

Filling a field now goes through core/forms' `fill_field` instead of
`set_value`: the answer is read in the field's format, checked by its
checks (a refusal shows the form's own message beside the field, as other
refusals do), and every calculated field is worked out again. The totals
that changed are patched into the list at once and said to screen readers
("Total due is now 1,234.50."), before the engine's own reading arrives.

The banner that said Letters doesn't run a form's calculations is gone,
because it now runs the standard ones. Forms with scripts outside what the
engine understands show its own sentence naming the fields instead.

Prepare Form is one section with a segmented control rather than several
sections, so the Fill & Sign tab keeps one line per tool. Its first pane,
Rules, edits one field's calculation (sum, product, average, smallest,
largest or a formula), format (number, percent, date, time, special,
pattern) and checks (required, a range, most characters, a condition with
its message). `formRules.ts` holds the draft and turns it into the engine's
model; it refuses a range together with a condition because a pdf field has
one validate action. The panel says plainly, under "Not offered", that
there are no submit buttons and no fields that show or hide, in the
engine's own sentences.

## 2026-11-12: Prepare Form's Order, Arrange and Values panes

Order sets the tab order of a page: rows, columns, the page's tags, or a
list to drag. Dragging needs a pointer, so each row also moves with Alt and
the arrows, and has small up and down buttons. Arrange groups top-level
fields under one name and puts one field on several pages (typed as page
ranges, like the page tools). Values sets what a checkbox or radio button
stands for, a field's default answer, resets fields to their defaults, and
adds Clear Form and Print buttons at the bottom right of the page being
read, where Edit Fields can move them. The engine's sentence about why
there is no submit button sits under the buttons.

## 2026-11-12: Form Data

A Form Data section exports a form's answers as FDF, XFDF, CSV or JSON
(the kind chosen sets the save panel's filter, and the shell reads the kind
back from the file's name), imports answers from any of them, and merges
many filled copies into one CSV with a File column. An import is one undo
step and says how many answers went in and which were left out and why, in
the engine's sentences.
