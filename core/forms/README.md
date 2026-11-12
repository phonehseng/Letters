# letters-forms

Form fields and signatures for Letters: reading, filling, adding, finding,
flattening, signature pictures and certificate-based digital signatures.
Everything runs on this computer. Nothing here uses the network.

## How to call it

Form operations take a `lopdf::Document` (lopdf 0.47) already in memory and
change it in place; the shell saves it afterwards. Signing and checking
signatures take the file's bytes instead, because a signature covers exact
bytes. Every parameter and result type is serde with camelCase names, so the
shell can pass them straight to the interface as JSON.

- Pages are counted from 0.
- Positions (`VisibleRect { x, y, width, height }`) are in points from the
  top left of the page as the reader sees it, y going down, after the page's
  turn (`/Rotate`, any multiple of 90) and cut (`/CropBox`, clipped to the
  `/MediaBox`, any origin).
- Errors are `FormError`, whose `to_string()` is a sentence for the person
  using the app.
- Encrypted documents are refused with "This PDF is locked with a password.
  Unlock it first, then fill or sign it." XFA forms are refused too.

## Operations

| Call | What it does to the file |
|---|---|
| `fields(&doc)` | Reads nothing into the file. Returns every field: full name, kind, widgets (page and rect), value, default, options, flags, max length, comb, tooltip, date format, alignment, font size, signed. |
| `set_value(&mut doc, name, &FieldValue)` | Checks the answer, writes `/V` (and `/I` for lists, `/AS` for buttons) and draws new appearance streams for the field's widgets. |
| `add_field(&mut doc, &NewField)` | Adds a field and its widget(s), creating `/AcroForm` with Helvetica (`/Helv`) in `/DR` if needed, and draws them. Returns the name used. |
| `update_field(&mut doc, name, &FieldChanges)` | Renames, moves, resizes, moves to another page, changes options, flags, tooltip, font size, alignment or date format, then redraws. Returns the new full name. |
| `remove_field(&mut doc, name)` | Removes the field, its widgets from the pages, and parents left empty. |
| `detect_fields(&doc, page)` | Reads the page's drawing and returns `DetectedField` proposals. Changes nothing. The interface confirms each one and calls `add_field`. |
| `flatten(&mut doc, &FlattenOptions)` | Paints each field's appearance into its page's content and removes the fields (and an empty `/AcroForm`). Returns the names flattened. `FLATTEN_EXPLANATION` is the one sentence for the interface: *Flattening makes your answers a permanent part of the page: they look exactly the same, but can no longer be changed as form fields.* |
| `redraw_fields(&mut doc, only_missing)` | Draws appearance streams again from the values, for files that leave drawing to the reader (`/NeedAppearances`). |
| `place_signature_image(&mut doc, &SignatureImage)` | Puts a PNG (typed or drawn signature, made by the interface) on a page, keeping its transparency through a soft mask: as page content, or as a stamp annotation. Not a digital signature. |
| `create_self_signed_certificate(&CertificateRequest)` | Makes an RSA 2048 or P-256 key and a self-signed certificate (subject includes `OU=Self-signed`), packed as a password-protected PKCS#12 (.p12). |
| `sign(&bytes, &SignRequest) -> Vec<u8>` | Signs with a PKCS#12 certificate into an existing empty signature field, a new one, or invisibly. Appends an incremental update; the original bytes are unchanged. |
| `verify(&bytes) -> Vec<SignatureReport>` | Checks every signature: who, when, digest, signature, changes after signing, certificate details. Changes nothing. |

`FieldValue` in JSON: `{"kind":"text","value":"Ada"}`,
`{"kind":"checked","value":true}`, `{"kind":"selected","value":["Art"]}`.
Text also chooses a radio button or dropdown option (by value or label), and
`checked: false` clears a radio group.

## What is guaranteed

- Appearance streams are always drawn by Letters. `/NeedAppearances` is never
  relied on (and never set). Fonts come from the field's `/DA`, looked up in
  `/AcroForm /DR`; when that font can't draw the text, Helvetica is used.
- Text: the `/DA` size, or a size that fits when it is 0 (single line: fits
  height and width, 4 to 24 pt; multiline: shrinks from 12 pt until the text
  fits, down to 4 pt). Alignment from `/Q`. Multiline text wraps at spaces,
  at typed line breaks, and inside words that are too long for a line. Line
  breaks typed into a multiline field (`\n`, `\r\n` or `\r`) are kept and
  stored as `\n` in `/V`; in a one line field each becomes a space. Comb
  fields put one character in each of `/MaxLen` cells. Text is clipped to the
  box. Password fields show asterisks.
- Checkboxes and radio buttons: `/V` holds the on-state name and every
  widget's `/AS` is set to match, using the export values of `/Opt` when the
  field has them. Their existing drawings are kept; missing ones are drawn
  (as vector shapes, so they look the same everywhere) in the `/MK /CA` style.
- Lists and dropdowns store the export value and show the label.
- Date fields are text fields with Acrobat's `AFDate_FormatEx("pattern")`
  script. Letters reads the pattern, never runs the script, and accepts
  `2026-11-11` or a date already in the field's pattern. Patterns: `yyyy yy
  mmmm mmm mm m dd d` and literal characters.
- Validation before anything changes: read-only, required (empty text, no
  choice, unticked), max length, choice not offered (unless the dropdown is
  editable), more than one choice where only one is allowed, characters
  outside WinAnsi.
- Field trees: `/Kids`, merged field-widget dictionaries, inherited `/FT /Ff
  /V /DV /DA /Q /MaxLen /Opt`, and `/DA /Q` from the form.
- New fields: unique names (`name`, `name_2`...), dots refused (they join name
  parts), `/MK /R` set to the page's turn so the drawing reads upright, and the
  widget put into `/Annots` in reading order with `/Tabs /R`.
- Flattening keeps the page looking the same: the tests render before and
  after and allow at most 20 differing pixels (anti-aliasing) on a letter page,
  upright and turned.
- Signatures: tests check sign then verify; one changed byte is caught;
  appended updates are reported; two signatures in a row both verify; RSA
  (PKCS#1 v1.5), RSA-PSS (when the key's algorithm is id-RSASSA-PSS) and
  ECDSA P-256 keys; files with cross-reference streams; certificate files made
  by OpenSSL; `openssl cms -verify` accepts the signatures; PDFium opens and
  draws the signed files.

## The digital signature format

- `/Filter /Adobe.PPKLite`, `/SubFilter /adbe.pkcs7.detached`, a CMS
  SignedData (RFC 5652) over the two `/ByteRange` parts of the file, SHA-256.
- Signed attributes: content type, message digest, signing time, and ESS
  signing-certificate-v2 (RFC 5035).
- Why not `ETSI.CAdES.detached`: PAdES baseline signatures may not carry the
  signing-time attribute, and without a time stamp authority Letters can't
  reach the PAdES levels above B-B anyway. `adbe.pkcs7.detached` is accepted by
  every reader and still names the exact certificate through ESS.
- The signer's certificate and the rest of the chain in the PKCS#12 are
  embedded. `/M`, `/Name`, `/Reason`, `/Location`, `/ContactInfo` are written
  in the signature dictionary. `/AcroForm /SigFlags` is set to 3.
- Each signature is an incremental update: new and changed objects, a new
  cross-reference section (a table, or a stream when the file's last section
  was a stream) and a trailer with `/Prev`. Earlier signatures stay valid.
- A visible signature shows the optional picture and "Digitally signed by …",
  the date (UTC), reason and location.

## Known limits

Fonts and text
- Widths are known for Helvetica, Helvetica-Bold, Times-Roman and Courier
  (from Adobe's Core 14 AFM files). Oblique, italic and Times-Bold use the
  nearest of these. Other simple fonts are measured with their `/Widths`.
- Text outside WinAnsi (Western European letters) is refused with a clear
  message; there is no font embedding yet.
- A field whose `/DA` font is a Type0 (two-byte) font is drawn in Helvetica.
- Rich text (`/RV`) is dropped when a value is set and never drawn.
- Field-level `/DR` is ignored; only `/AcroForm /DR` is read.
- Comb fields ignore `/Q`. List boxes draw from `/TI` and do not scroll to
  the chosen option.

Fields
- JavaScript never runs: no calculations, no formats other than dates, no
  validation scripts. Date patterns with times (`HH:MM`) are not supported.
- Push buttons are read (kind `button`) but can't be filled or added.
- `remove_field` leaves the removed widgets' drawings in the file, because
  other widgets may share them. They are unused and harmless.
- `update_field` can't change a radio group's buttons or a field's kind.

Finding fields (`detect_fields`)
- It reads vector drawing and text only. A scanned form (a picture of a page)
  gives no proposals; that needs OCR.
- Tested on synthetic layouts, upright and turned: lines to write on (stroked
  or drawn as thin filled rectangles), typed underscores, boxes and table
  cells (from rectangles or loose lines, with labels inside or beside them),
  small squares, small circles, combs (touching boxes, one box with
  dividers, or separate boxes with even gaps up to about a box width), and
  rules after a label ending in a colon, also when the rule runs under the
  label. On those, every proposal is within 2 points of the expected
  rectangle and carries the expected label; a page of plain text gives none.
- Rows of separate boxes stay checkboxes when there is text between them,
  right above or below them (a rating scale), or right after the last one. A
  comb of separate boxes is proposed half a gap wider on each side, so that
  its equal cells line up with the boxes.
- A rule counts as underlining (no field) when text covers half of it or
  more, unless that text starts the rule and ends in a colon (with 24 points
  of rule left) or leaves at least 36 points (and a third of the rule) empty.
- Real forms will do worse. Dashed or dotted lines (gaps over 1 point) and
  dot leaders are not found. Rounded boxes count as curves, not boxes. Merged
  table cells may be missed or split. Decorative rules may be proposed as
  lines to write on (with confidence 0.5). Clipping is ignored, so hidden
  drawing can produce proposals. Labels come from text with WinAnsi codes or a
  `/ToUnicode` map; text in other two-byte fonts gives no label.
- Places that already have a widget are not proposed again.

Flattening
- Signature fields are kept unless `includeSignatures` is set.
- Hidden widgets are removed without being painted.
- Flattening (like any edit saved by rewriting the file) breaks existing
  digital signatures. Only `sign` writes incrementally.

Signatures
- Signing uses SHA-256 only; checking also reads SHA-384 and SHA-512.
- Keys: RSA (any size) and ECDSA on P-256. Other curves and Ed25519 are refused.
- No time stamps from a time stamp authority, no long-term validation
  (PAdES LT/LTA) and no revocation or trust-list checks: all need a server.
  The signing time is this computer's clock.
- No certification (DocMDP) signatures and no field locking (`/Lock`).
- Invisible signatures sit on the first page.
- `/ByteRange` has room for files up to 9,999,999,999 bytes.

Checking signatures (`verify`)
- Letters checks that the signature is correct and the signed bytes
  unchanged. It does not check certificate authorities, trust lists or
  revocation, so it can't confirm who the signer is. Every report says so
  (`trust` is `selfSigned` or `notChecked`, never trusted).
- "Changed after signing" means bytes follow the signed range, or the byte
  range no longer fits the file at all (it was rewritten, for example saved
  by a program that doesn't append). In the rewritten case the signer, time
  and certificate are still read from the signature and the signature over
  its own attributes is still checked, but the document can't be compared,
  so `valid` is false. Letters says whether each later update ends with
  another signature, but does not compare what those updates changed.
- `summary` is plain sentences for the interface, such as "Signed by Ada
  Lovelace on 3 March 2026. The document was changed after signing, so this
  signature no longer covers it." followed by what Letters can't check.
- Only signatures reachable from `/AcroForm /Fields` are found. Only the first
  signer in a CMS structure is checked. BER-encoded (indefinite length)
  signatures from other tools can't be read. Certificate chains are listed but
  not checked, apart from whether a certificate signed itself.

Certificates (`create_self_signed_certificate`)
- RSA 2048 (default) or P-256. The PKCS#12 is protected with AES-256 and
  PBKDF2 with an HMAC-SHA-256 check (the modern default). Very old software
  that only reads RC2 or 3DES protected files can't open it.
- Self-signed means nobody vouches for the name: the result says so in
  `selfSigned` and `note`.

## Libraries and licenses

All direct dependencies are MIT or Apache-2.0 (most both): lopdf, serde,
thiserror, png, rsa, p256, sha2, der, spki, x509-cert, cms, const-oid, rand
(the RustCrypto project) and p12-keystore (which brings x509-parser). Their
own dependencies add BSD-3-Clause (subtle, alloc-no-stdlib, encoding_rs in
part), 0BSD, Zlib, Unlicense and Unicode-3.0 (unicode-ident) options, all
permissive. Nothing restricts distributing the app.

## Tests

`cargo test -p letters-forms` (with `CARGO_TARGET_DIR` set to your own
folder). The render tests use PDFium from `vendor/pdfium` (run
`tools/fetch-pdfium` first). The OpenSSL cross-checks run when `openssl` is on
the path or in the usual Windows places, and are skipped with a message
otherwise.
