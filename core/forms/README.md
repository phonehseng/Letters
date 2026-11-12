# letters-forms

Form fields and signatures for Letters: reading, filling, adding, finding,
flattening, form logic (calculations, formats, checks), building forms,
form data in and out, signature pictures, certificate signatures with time
stamps, certification and field locks, and an audit trail kept inside the
PDF. Everything runs on this computer. The only network use is a time
stamp request, and only to a time stamp authority the person configured,
sending only a hash.

## How to call it

Form operations take a `lopdf::Document` (lopdf 0.47) already in memory and
change it in place; the shell saves it afterwards. Signing, time stamps,
the audit trail and checking signatures take the file's bytes instead,
because a signature covers exact bytes. Every parameter and result type is
serde with camelCase names, so the shell can pass them straight to the
interface as JSON.

- Pages are counted from 0.
- Positions (`VisibleRect { x, y, width, height }`) are in points from the
  top left of the page as the reader sees it, y going down, after the page's
  turn (`/Rotate`, any multiple of 90) and cut (`/CropBox`, clipped to the
  `/MediaBox`, any origin).
- Errors are `FormError`, whose `to_string()` is a sentence for the person
  using the app.
- Encrypted documents are refused with "This PDF is locked with a password.
  Unlock it first, then fill or sign it." XFA forms are refused too.
- Round one's API is unchanged; everything below marked *(new)* was added
  beside it. New error cases are new `FormError` variants.

## Operations

### Fields (round one)

| Call | What it does to the file |
|---|---|
| `fields(&doc)` | Reads every field: full name, kind, widgets, value, default, options, flags, max length, comb, tooltip, date format, alignment, font size, signed. |
| `set_value(&mut doc, name, &FieldValue)` | Checks the answer, writes `/V` (and `/I`, `/AS`) and draws new appearance streams. Doesn't run form logic. |
| `add_field`, `update_field`, `remove_field` | Add, change and remove fields. |
| `detect_fields(&doc, page)` | Proposes fields on flat forms (`DetectedField`). Changes nothing. |
| `flatten(&mut doc, &FlattenOptions)` | Paints fields into the page and removes them. |
| `redraw_fields(&mut doc, only_missing)` | Draws appearance streams from the values. |
| `place_signature_image(&mut doc, &SignatureImage)` | A PNG signature picture (not a digital signature). |

### Form logic *(new)*

| Call | What it does |
|---|---|
| `form_logic(&doc) -> FormLogic` | Reads every field's calculation, format and checks into the declarative model (`FieldLogic`), the calculation order, problems (`CalculationIssue`: circles, unknown fields, out of order), and `customScriptNote` naming fields whose scripts Letters leaves alone. Changes nothing. |
| `fill_field(&mut doc, name, &FieldValue) -> FillReport` | Fills a field the way a form app does: the answer is read in the field's format, checked by its validations (refused with the form's own message or a plain sentence, as `FormError::AnswerRefused`), stored raw, shown formatted, then every calculated field is worked out again in order. `changed` lists the field and each calculated field that changed, with its raw `value` and formatted `shown` text. Nothing changes when refused. |
| `recalculate(&mut doc) -> FillReport` | Works out every calculation again and redraws formatted fields. |
| `set_field_logic(&mut doc, name, &FieldLogicChanges) -> FieldLogic` | Sets a field's calculation, format and checks (replacing what it had), writes them as standard actions, keeps `/CO` in step, recalculates, and returns the logic as read back from the file. |
| `set_calculation_order(&mut doc, &[name])` | Sets `/CO`. |

The model, in JSON:

- `Calculation`: `{"type":"aggregate","function":"sum"|"product"|"average"|"minimum"|"maximum","fields":[...]}`,
  `{"type":"formula","formula":"price * qty"}` (letters notation, below), or
  `{"type":"script","source":"..."}` (a script in the safe subset that is
  more than one formula).
- `FieldFormat`: `number {decimals, separator, negative, currency, currencyBefore}`,
  `percent {decimals, separator}`, `date {pattern}`, `time {pattern}`,
  `special {kind: zipCode|zipPlus4|phone|socialSecurity}`, `mask {mask}`.
  `separator` is `commaDot` (1,234.56), `dot`, `dotComma` (1.234,56),
  `comma`, `apostropheDot`; `negative` is `minusSign`, `red`,
  `parentheses`, `redParentheses`.
- `Validation`: `range {min, max}`, `required`, `maxLength {max}`,
  `condition {condition, message}` (letters notation, `value` is the
  field's own answer: `value <= budget`), `script {source}`.

Letters notation: field names, numbers, `+ - * /`, brackets, percentages
(`15%` is 0.15), comparisons (`< <= > >= = !=`), `and`, `or`, `not`,
`? :`, and `round floor ceil min max abs number`. Names with spaces or dots
in odd places go in double quotes: `"Line 1" * 2`. Fields are read as
numbers (an empty field is 0), as the javascript it is written as does.

### Building forms *(new)*

| Call | What it does |
|---|---|
| `set_tab_order(&mut doc, page, &TabOrder)` / `tab_order(&doc, page)` | `rows`, `columns`, `structure` (`/Tabs /R /C /S`) or `manual {fields}`; the page's `/Annots` is put in the same order, because readers that ignore `/Tabs` follow it. A manual order removes `/Tabs`. |
| `group_fields(&mut doc, group, &[names])` | Puts top-level fields under one parent: `street` becomes `address.street`. |
| `duplicate_across_pages(&mut doc, name, &[pages])` | One field (one answer) with a widget at the same place on each page. |
| `set_export_value(&mut doc, name, button, value)` | The value a checkbox or radio button stands for. |
| `set_default_value(&mut doc, name, Option<&FieldValue>)` / `reset_form(&mut doc, Option<&[names]>)` | `/DV`, and putting fields back to it. |
| `add_button(&mut doc, &NewButton)` | Reset (`/ResetForm`) and print (`/Named /Print`) buttons: standard actions, no scripts. |
| `set_field_appearance(&mut doc, name, &FieldAppearance)` | Border colour, width and style, fill, font (Helvetica, Helvetica Bold, Times, Courier), size, text colour, alignment, multiline, password, comb, rich text off. |

`NO_SUBMIT_NOTE`: there is no submit button. Sending answers needs a
server or a mail program; people save the filled form and send the file.

### Form data *(new)*

| Call | What it does |
|---|---|
| `export_data(&doc, DataFormat) -> bytes` | `fdf`, `xfdf`, `csv` (header row and one row) or `json` (one object; lists for multi-select). Hierarchical names become trees in FDF and XFDF. Checkboxes and radio groups give their export value or `Off`. |
| `import_data(&mut doc, bytes, DataFormat) -> ImportReport` | Fills the form; every answer goes through its field's format and checks. Unknown names, calculated fields and refused answers are listed in `skipped` with a sentence; the calculations run once at the end. Empty answers for empty fields change nothing. |
| `merge_form_data(&[FilledCopy]) -> MergedData` | Many filled PDFs into one table: a `File` column, then every field seen. `csv` is the table; files that can't be read are in `skipped`. A row of the merged CSV imports back (the `File` column is ignored). |

### Signatures

| Call | What it does |
|---|---|
| `create_self_signed_certificate(&CertificateRequest)` | RSA 2048 or P-256 key and a self-signed certificate in a PKCS#12. |
| `sign(&bytes, &SignRequest)` | As round one: signs as an incremental update. |
| `sign_with_options(&bytes, &SignRequest, &SigningOptions)` *(new)* | `timestamp` (a `TimestampAuthority { url, username, password }`), `certify` (`noChanges`, `formFillingAndSigning`, `formFillingSigningAndComments`), `lock` (`FieldLock`: `{"action":"all"}`, `{"action":"include","fields":[...]}`, `{"action":"exclude",...}`), `validation` (`ValidationData` the caller already has), `followSigningOrder`, `auditActor` (records the signature in the audit trail). |
| `add_document_timestamp(&bytes, &TimestampAuthority)` *(new)* | A `/DocTimeStamp` (`/SubFilter /ETSI.RFC3161`) over the whole file so far, as an incremental update. |
| `add_validation_data(&bytes, &ValidationData)` *(new)* | A document security store (`/DSS`): every certificate the signatures and their time stamps carry, plus the CRLs and OCSP responses given, with a `/VRI` entry per signature. Nothing is fetched. |
| `set_signature_lock(&mut doc, field, Option<&FieldLock>)` / `signature_lock` *(new)* | Which fields a signature field locks once signed (`/Lock`). |
| `set_signing_order(&mut doc, &[fields])` / `signing_workflow(&doc)` *(new)* | The order signature fields are signed in (the order of `/AcroForm /Fields`), and who has signed and who is next. |
| `save_incremental(&original, &doc)` *(new)* | Saves a changed document by appending only what changed. **Use it for every save of a signed document**; a normal save rewrites the file and breaks every signature. |
| `verify(&bytes)` | As round one, and now also checks document time stamps. |
| `verify_document(&bytes) -> DocumentVerification` *(new)* | Every signature's report, plus per signature: its kind (`approval`, `certification`, `documentTimestamp`), its time stamp (`TimestampInfo`), `timeSource` (`timestampAuthority` or `signerClock`) and `timeNote`; the certification level; every revision of the file with what it changed (`RevisionReport`, `Change` of kind `fieldValue`, `signature`, `documentTimestamp`, `formDefinition`, `annotation`, `pageContent`, `validationData`, `auditTrail`, `metadata`, `other`); and per signature `SignatureChanges` with a `verdict` (`unchanged`, `asIntended`, `notAllowed`, `noLimits`) and a sentence: "Filled in and signed as intended: …" or "Changed in ways the author did not allow: …". |
| `revisions(&bytes)` *(new)* | The revisions and their changes on their own. |

`TIMESTAMP_DISCLOSURE`, word for word in the interface before anything is
sent: *To add a trusted time, Letters sends the time stamp server you
chose a fingerprint (a SHA-256 hash) of the signature, or of the document
for a document time stamp. The document itself never leaves this computer,
and a fingerprint can't be turned back into it.* `CLOCK_TIME_NOTE`: *The
time comes from the signer's computer clock.*

### Audit trail *(new)*

| Call | What it does |
|---|---|
| `append_audit_entry(&bytes, actor, &AuditEvent)` | Adds an entry as an incremental update. Events: `created`, `prepared {note}`, `signed {field, signer, certificateSubject, certificateFingerprint, signingTime, timeSource, reason, location}`, `saved`, `timestamped {authority}`, `note {text}`. `actor` is the name the person set in Letters. |
| `audit_trail(&bytes) -> AuditTrailReport` | The entries, each checked (`hashMatches`, `chainIntact`, `documentMatches`, `sealed`), a `verdict` (`noTrail`, `intact`, `broken`), problems in sentences, a summary, and the note below. |
| `add_audit_page(&bytes)` | Adds readable pages listing the entries (a copy for reading; the embedded file is the record). Pages are page content, which a certification forbids: add them before certifying or signing. |

**`AUDIT_TRAIL_NOTE`, shown wherever the trail is:** *Letters records what
happens to this document in Letters on this computer: who prepared, signed
and saved it, and when. It can't know who opened or viewed the file
anywhere else, and it doesn't use a tracking service.* There is no
tracking of opening or viewing, by design: the brief rules out servers and
response tracking, and a PDF can't report its own opening without one.

## What is guaranteed

Round one's guarantees all still hold (appearance streams always drawn by
Letters; text, checkbox, list and date behaviour; validation before
changes; field trees; flattening keeps the look; signatures verify, catch
one changed byte, survive further signatures, and are accepted by
`openssl cms -verify`).

Form logic
- The standard actions are recognised by their arguments:
  `AFSimple_Calculate` (array or comma string of names; a name standing for
  a group counts every field under it), `AFNumber_Format/_Keystroke`,
  `AFPercent_Format/_Keystroke`, `AFDate_FormatEx/_KeystrokeEx/_Format(n)`,
  `AFTime_Format/_Keystroke(n)` and `_FormatEx`, `AFSpecial_Format/_Keystroke(0-3)`,
  `AFSpecial_KeystrokeEx(mask)`, `AFRange_Validate`.
- Other scripts are read by a hand-written parser of a small subset:
  `event.value =`, `event.rc =`, `var`, `if/else`, `app.alert(...)`,
  `this.getField("x").value` (and `.valueAsString`), numbers, text,
  `+ - * / %`, comparisons, `&& || !`, `? :`, `Math.round/floor/ceil/min/max/abs`,
  `Number()`, `AFMakeNumber()`, `parseFloat()`. No loops, no functions of its
  own, no other property access. Limits: 8,192 characters, 600 tokens, 48
  levels of nesting, 20,000 steps. Anything else is left untouched and
  reported with the field names: "This form has a custom script Letters
  doesn't run: …".
- Calculations run in `/CO` order (then any calculated field `/CO` forgot),
  as other apps do; a field that reads a later one is reported
  `outOfOrder`. Circles (including a field reading itself) are found with
  Tarjan's algorithm, reported, and those fields are left alone. A result
  that isn't a number (dividing by zero) leaves the field empty and is
  reported. Calculated fields are written even when read-only.
- `/V` keeps the raw value (`1234.5`, a percentage as `0.125`, phone digits
  `0215551234`); the drawing shows it formatted (`1.234,50 EUR`,
  `12.5%`, `(021) 555-1234`); red negative styles draw red. Dates, times
  and masks are stored the way they are shown, as acrobat stores them.
- Typed numbers are read in the field's own separator style or plainly,
  with the currency and spaces ignored and brackets meaning negative.
  Percentages accept `12.5%` (0.125) or a fraction. Times accept `14:30`,
  `2:30 pm`, `2 pm`, with seconds.
- Authoring writes exactly those standard calls (formulas as `event.value =
  AFMakeNumber(this.getField("a").value) * …` with acrobat's simplified
  field notation comment `/** BVCALC a * b EVCALC **/`), and conditions as
  `if (!(…)) { app.alert("…"); event.rc = false; }`. Tests author every kind,
  save, reload and compare the model field by field.

Signing extras
- Time stamps: the request asks for the authority's certificate and a
  random nonce; the answer must have status granted, the same nonce, a
  message imprint matching the hash sent, a correct signature by a
  certificate marked for time stamping, and the ESS signing-certificate
  attribute (rfc 3161 2.4.2). Anything else refuses the signature with a
  sentence (`FormError::TimestampFailed`), and nothing is signed. HTTP
  basic auth is supported; a refused password says so.
- Certification: only the first signature may certify (`CertifyFirst`);
  a level 1 document refuses further signatures (`CertifiedNoChanges`).
  Locked fields become read-only in the signing update; signature fields
  are never locked, so later signers can sign.
- Changes are judged per revision: a change in revision *t* is not allowed
  when a certification made before *t* doesn't allow its kind (level 1:
  nothing but evidence; level 2: answers, signatures, metadata; level 3:
  also annotations), or a signature made before *t* locked its field. The
  security store, document time stamps and Letters' own audit trail are
  always allowed (etsi en 319 142-1 5.4 allows the first two even at
  level 1).

Audit trail
- Each entry holds the SHA-256 of the whole file as it was just before the
  entry was added and the hash of the entry before; its own hash covers all
  of it (JSON in a fixed field order). The checker also compares every
  revision's copy of the trail with the next, so removing or rewriting
  older entries in a later save shows ("A later save changed or removed
  audit entries…"), and reports which entries a valid signature or document
  time stamp made later seals. Entries after the last seal are named as
  unsealed: until the next signature they could be rewritten consistently
  without it showing.

## How it is tested

`cargo test -p letters-forms` (set `CARGO_TARGET_DIR`). 167 tests: 73 unit
tests and 94 integration tests in `tests/`.

- `logic.rs`: hand-built forms using each standard action; SUM, PRD, AVG,
  MIN, MAX; field arithmetic, ternaries and `if`/`var` scripts; the wrong and
  right `/CO` order (and fixing it); circles; every format with comma and
  apostrophe separators, currency before and after, red negatives; refused
  answers with the form's own message; scripts outside the subset kept in
  the file and reported; every kind of rule authored, saved, reloaded and
  compared; bad rules refused with sentences.
- `build.rs`: tab orders, groups, duplicate across pages, export and
  default values and reset, reset and print buttons, field looks, and form
  data round trips in FDF, XFDF, CSV and JSON, imports passing checks, and
  merging copies.
- `timestamp.rs`: a fake time stamp authority run inside the tests on
  127.0.0.1 (std `TcpListener`, a P-256 key and a certificate made in the
  test). Signature and document time stamps verify; `openssl ts -verify`
  accepts both kinds of token (`-digest` for signature time stamps,
  `-data` for document time stamps) and `openssl ts -reply -text` reads
  them; tokens made by `openssl ts -reply` (a second, independent
  authority) are accepted by Letters; wrong imprint, wrong nonce, refusal,
  missing ESS attribute, unreachable server and wrong password are each
  refused; one changed byte breaks a document time stamp. No test contacts
  the internet.
- `certify.rs`: each certification level with allowed and forbidden later
  changes (filling, countersigning, comments, page drawing, validation
  data); certification only first; no limits without certification; a two
  signer workflow in order, locked fields refusing changes, and tampering
  with a locked answer reported against both signatures; the security
  store's contents and VRI key.
- `audit.rs`: a full history intact and sealed; changing any entry breaks
  the trail and the seal; taking an entry out in a later save shows;
  entries after the seal are marked; audit entries are allowed after a
  certification; the readable page.
- The round one tests (`fill`, `add`, `detect`, `flatten`, `sign`, `stamp`)
  all still pass, including the OpenSSL cross-check of signatures.

The render tests use PDFium from `vendor/pdfium`. The OpenSSL cross-checks
run when `openssl` is on the path or at `C:\msys64\ucrt64\bin\openssl.exe`
and are skipped with a message otherwise.

## Known limits

Form logic
- Not a JavaScript engine. Format and keystroke scripts other than the
  standard ones aren't run. Conditional visibility (showing or hiding
  fields from a script) is out of scope (`CONDITIONAL_VISIBILITY_NOTE`).
- No PDF app that runs form scripts was reachable on this machine through
  automation (no Acrobat or Foxit is installed; Edge's viewer runs some
  scripts but can't be driven headless), so authored forms were checked by
  reading their actions back, not by running them in another app.
- A recognised formula runs as numbers (`AFMakeNumber`); a hand-written
  script without `AFMakeNumber` that joins text with `+` on empty fields
  would show "3" where acrobat shows "3" as text: the same on screen.
- `set_field_logic` replaces a field's logic; leaving out max length
  removes it (and so a comb). Calculations of non-text fields are skipped.
- `redraw_fields` (round one) draws raw values; `recalculate` draws the
  formatted ones.

Building and data
- No submit buttons (needs a server), no barcode fields.
- Fonts for field looks are the four standard fonts; no font embedding.
- `group_fields` only groups top-level fields. XFDF reading understands
  `<field>` and `<value>` only (no annotations or rich values). FDF with
  a name tree of `/Kids` for the attachment list isn't read. CSV joins
  multi-select answers with "; ".

Signatures
- Signing with SHA-256 only; RSA and P-256 keys only.
- Revocation data is never fetched; Letters checks no trust lists,
  revocation or chains, and every report says so (`TRUST_NOTE`). The DSS
  `/VRI` key is the SHA-1 of the stored `/Contents` bytes, padding
  included, as iText and pyHanko write it.
- Revision analysis sorts changed objects by what they are and what refers
  to them. It is tested on the kinds of change Letters and common tools
  make; unusual files may be described as "other parts of the file were
  changed". A change to a field's /Ff alone is read as a signature's lock.
- Signature appearance options beyond round one's (custom layouts, logo
  placement), several signers in one field, PKCS#11 tokens and the OS
  certificate store are out of scope for now. A pure-Rust route to the
  Windows certificate store exists on this GNU toolchain: the `windows`
  crate (MIT or Apache-2.0, already in the workspace through Tauri, with
  `windows_x86_64_gnu` import libraries, so no MSVC) exposes
  `CertOpenSystemStoreW`, `CertEnumCertificatesInStore`,
  `CryptAcquireCertificatePrivateKey` and `NCryptSignHash`, which is enough
  to sign with a key that never leaves Windows. It isn't built.
- The audit trail records only what happens in Letters on this computer.
  Its names are the ones the person typed; Letters can't confirm them.
  Converting a document with a trail to PDF/A-1 or PDF/A-2 removes the
  attachment (only PDF/A-3 keeps it). Other readers may describe the
  trail's update after a certification as an attachment change.

Fonts and text, fields, finding fields, flattening: as round one (Western
European text only, rich text dropped, push buttons only as reset/print,
`detect_fields` on vector drawing only).

## Libraries and licenses

All direct dependencies are MIT or Apache-2.0 (most both): lopdf, serde,
serde_json, thiserror, png, rsa, p256, sha2, sha1, der, spki, x509-cert,
cms, const-oid, rand (the RustCrypto project), p12-keystore, and ureq with
rustls (already used by letters-fonts, so the same TLS stack and licences:
rustls is Apache-2.0/MIT/ISC, ring is Apache-2.0 and ISC with OpenSSL-style
parts, webpki-roots CDLA-Permissive-2.0 data). Their own dependencies add
BSD-3-Clause, 0BSD, Zlib, Unlicense and Unicode-3.0 options, all permissive.
Nothing restricts distributing the app. OpenSSL is used by tests only, when
installed, and is never linked.
