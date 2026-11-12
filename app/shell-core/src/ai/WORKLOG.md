# Worklog: app/shell-core/src/ai

## 2026-11-11 document text, plans and consent

Added `document_text.rs` (the structure analysis as pages of paragraphs with
rectangles, one per list item and table row so a citation outlines what it
quotes), `desk.rs` (plans wait under their disclosure id until the person's
Send, and run once) and `consent.rs` ("Don’t ask again" for one provider and
one document, in memory only), because core/ai needs paragraphs with places
and the shell must enforce the disclosure rule itself, not only the ui.

## 2026-11-11 keys and test connection

Added `keys.rs` and `connection.rs`. `ApiKey` keeps its text private to
core/ai, so a store outside that crate can't read a key back to save it;
typed keys go straight to the credential store through `TypedKeyStore`.
Test Connection sends one fixed sentence through the normal plan and run, so
it exercises the real path without any document text.
