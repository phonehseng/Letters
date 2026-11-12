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
