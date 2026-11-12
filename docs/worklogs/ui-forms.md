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

## 2026-11-12: signing options, Verify Signatures and the audit trail

Sign with a Certificate has Signing Options under a disclosure, so a plain
signature stays one click. Add a Trusted Time is only on when a server is
set under Settings, Signing (a new section there, with the engine's
disclosure); ticking it shows TIMESTAMP_DISCLOSURE word for word before
Sign can send anything, and the effect line names the server. Certify
offers the three levels in plain words ("No changes allowed", "Filling in
and signing allowed", "Filling in, signing and comments allowed") and is
off once the document has a signature, because only the first may
certify. Locks are None, All or the fields ticked; signature fields stay
open. Follow the signing order appears with two or more signature fields;
the order itself is set in Prepare Form's new Signers pane. Store the
certificates adds validation data; Record it in the audit trail is on by
default and shows AUDIT_TRAIL_NOTE as is.

Signatures became Verify Signatures: each signature's verdict sentence
(unchanged, changed as allowed, not allowed, changed with no limits),
where its time comes from with the engine's note, the certification and
what it allows, and every revision with its changes sorted into kinds.
Audit Trail shows the verdict, each entry and whether a later signature
seals it, starts a trail, and adds the readable pages, which are refused
once the document is signed because a certification forbids new pages.
The Privacy summary in Settings now lists trusted time stamps as the one
way signing can leave the computer.

## 2026-11-12: refusals that begin with the field's name

Seen in the app: form logic's refusals begin with the field's name, so
"this field takes a number" started with a small letter beside the field.
The sentence now starts with a capital when it began with the name.

## 2026-11-12: a refusal goes when the old answer is typed back

Seen in the app: after a refused answer, typing the stored answer back left
the refusal showing, because the editor sends nothing when the text equals
what is stored. It now sends it while a refusal is showing, and the session
clears the refusal for an unchanged answer.

## 2026-11-12: the banner reads the engine's verdict

With every save of a signed document appended after the signature, the
banner said "it has changed since" after any allowed change, such as the
audit trail entry signing adds. The overview now carries the engine's
verdict per signature (from verify_document), and a valid signature whose
later changes the author allowed reads "Valid, changed as allowed"; changes
that weren't allowed, or with no verdict, still read as changed.

## 2026-11-12: Work Out Totals Again, and no commands without a button

Rules has Work Out Totals Again for a form that arrives with totals that
don't match its answers. The commands for setting the calculation order and
adding a document time stamp had no place in the interface yet, so they
were taken out rather than left unused; core/forms has both when they get
one (a document time stamp also needs the disclosure shown first).
