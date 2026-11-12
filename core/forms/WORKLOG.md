# letters-forms worklog

Newest entry last. Each entry says what changed and why.

## 2026-11-12 round two begins: calculations without a script engine

Added `src/logic/`: a declarative model of form calculations, formats and
checks (`FieldLogic`), a reader that recognises acrobat's standard built-in
functions (AFSimple_Calculate, AFNumber_Format, AFPercent_Format,
AFDate_FormatEx, AFTime_Format, AFSpecial_Format, AFRange_Validate) in a
field's /AA actions and the /CO order, and an evaluator that recomputes
calculated fields in that order, finds circles, keeps raw values in /V and
draws formatted ones. Simple field arithmetic scripts are read by a small
hand-written parser (`script.rs`) and run by a step-limited interpreter
(`eval.rs`), because real forms mostly use these few shapes and a general
javascript engine would be a large, risky dependency. Anything outside the
subset is left untouched and reported with the field names
(`CUSTOM_SCRIPT_NOTE`). Authoring (`author.rs`) writes the same standard
actions back, so apps that run form scripts behave the same; formulas also
keep acrobat's simplified-notation comment. Round one's API is unchanged:
new errors are new variants, new behaviour is in new functions
(`fill_field`, `recalculate`, `form_logic`, `set_field_logic`).

## 2026-11-12 drawing a field with a shown value

`appearance.rs` gained `draw_shown_text`, so a field can show its value
formatted (and red for negative-in-red number formats) while /V keeps the
raw value, as iso 32000 12.7.5.3 intends for format actions.

## 2026-11-12 tools for building bigger forms

Added `src/build.rs`: tab order by rows, columns, structure or a manual
list (the annotation list is reordered too, because readers that ignore
/Tabs follow it); grouping fields under a shared parent name; showing one
field on several pages ("duplicate across pages" makes extra widgets of the
same field, so the answer is shared); export values; default values and
reset; reset and print buttons as standard /ResetForm and /Named actions (no
scripts); field looks (border, fill, font, colour, alignment, multiline,
password, comb, rich text off). A submit button is not offered because it
needs a server or a mail program (`NO_SUBMIT_NOTE`).

## 2026-11-12 form data in and out

Added `src/data.rs`: export and import as FDF, XFDF, CSV and JSON, and
`merge_form_data` to gather many filled copies into one CSV with a row per
file. FDF files have no cross-reference table lopdf can read, so one is
rebuilt in memory before loading; XFDF is read by a small reader that only
understands `<field name>` and `<value>`, which is all form data needs.
Imported answers pass through each field's format and checks; calculated
fields are skipped and worked out once at the end.

## 2026-11-12 split long files along their seams

No behaviour change; all tests green before and after. `appearance.rs`
(1,220 lines) became `appearance/` with `fonts.rs` (/DA and form fonts),
`widget.rs` (the widget box and storing drawings), `text.rs` (text
answers), `choices.rs` (checks, radios, lists) and `mod.rs` (redrawing a
field from its value). The script language became `lex.rs`, `parse.rs`,
`statements.rs`, `print.rs` and `script.rs` (the tree and helpers), with
the long lexer and statement functions broken into one function per kind
of token or statement. `data.rs` became `data/` (fdf, xfdf, csv and json)
and `build.rs` became `build/` (tab order, groups, values, buttons,
looks). Long functions (`set_field_appearance`, `read_xfdf`) were split
into named steps, because the readability rules ask for functions that fit
on a screen.

## 2026-11-12 time stamps from an authority the person chooses

Added `signing/timestamp.rs` and `signing/tsp.rs`: rfc 3161 requests and
responses, and checking a token's hash, the authority's signature, its
time-stamping key usage and the ess signing certificate attribute.
`sign_with_options` can add a signature time stamp (the
id-aa-signatureTimeStampToken unsigned attribute over the signature value,
rfc 3161 appendix a) and `add_document_timestamp` adds a /DocTimeStamp
(/SubFilter /ETSI.RFC3161) as an incremental update. No authority is built
in: the caller passes the url and optional basic-auth credentials, and only
a hash is sent (`TIMESTAMP_DISCLOSURE`). `verify_document` reports where
each signature's time comes from; without a valid token it says the time
comes from the signer's computer clock. `ureq` (already used by
letters-fonts, so no new licences) does the http. The ess attribute check
was added after `openssl ts -verify` refused the first fake authority's
tokens for lacking it; rfc 3161 2.4.2 requires it, so our checker now does
too. `sign.rs` was split: dates (`dates.rs`), the visible look (`look.rs`)
and the placeholder mechanics (`placeholder.rs`) moved out so document time
stamps share them; `verify.rs` lost the signature algorithms
(`algorithms.rs`) and the report sentences (`report_text.rs`).
## 2026-11-12 certification, field locks, signing in turn, the security store

`SigningOptions` gained `certify` (docmdp levels 1 to 3, written as a
/Reference on the signature and /Perms in the catalog), `lock` (fieldmdp;
a signature field's own /Lock, set with `set_signature_lock`, wins) and
`follow_signing_order` (signature fields are signed in form order, set with
`set_signing_order`; `signing_workflow` reports who has signed and who is
next). Locked fields become read-only in the signing update, as acrobat
does. `add_validation_data` writes a /DSS with every certificate the
signatures and their tokens carry plus the crls and ocsp responses the
caller already has; Letters fetches nothing. `save_incremental` saves a
changed document by appending only what changed, so earlier signatures
stay valid. `revisions` splits a file at each %%EOF and sorts every changed
object into a kind of change by what it is and what points at it;
`verify_document` judges the changes after each signature against the
certification level and field locks ("filled in and signed as intended"
versus "changed in ways the author did not allow"). The security store,
document time stamps and Letters' own audit trail are always allowed, as
etsi en 319 142-1 5.4 allows the first two even at level 1.
## 2026-11-12 an audit trail inside the pdf

Added `src/audit/`: an append-only list of what happened to the document
in Letters (created, prepared, signed, saved, time stamped, notes), kept as
the embedded file letters-audit-trail.json (listed in /EmbeddedFiles and as
an associated file, which pdf/a-3 asks for). Each entry records the sha-256
of the file as it was and of the entry before, and every new entry is its
own incremental update, so `audit_trail` can check each entry's hash, the
chain, the recorded file hashes against the real revisions, that no later
revision took entries out, and which entries a valid signature or document
time stamp seals. `sign_with_options` records a signature when
`audit_actor` is given. `add_audit_page` adds readable pages (a copy; the
file is the record). `AUDIT_TRAIL_NOTE` says plainly that Letters only
knows what happens on this computer and uses no tracking service; nothing
about opening or viewing is recorded, because the brief rules out servers.
## 2026-11-12 readme for round two

The README now lists every new call as the shell should make it, what is
guaranteed, how each part is tested, every limit found on the way, the
result of a brief look at the Windows certificate store (a pure-Rust route
through the windows crate exists on the gnu toolchain; not built), and the
plain statement that the audit trail only knows what happens in Letters on
this computer. `logic/format.rs` was split into `numbers.rs` and `masks.rs`
to stay under the readability limit; no behaviour changed.
