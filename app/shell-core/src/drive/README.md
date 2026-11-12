# Google Drive (app/shell-core/src/drive)

Saving a writer document to Google Drive: the Drive folder on this computer,
or direct upload with Google's sign-in. Why each piece changed is in
docs/worklogs/shell-core-drive.md.

| File | What it is for |
|---|---|
| `fake_google.rs` | a stand-in for google's sign-in and drive endpoints, on this computer, so the whole drive flow is tested without ever contacting google |
| `local.rs` | "Save to Google Drive folder" |
| `mod.rs` | saving to google drive |
| `oauth.rs` | signing in to google for drive uploads, the way google asks installed apps to do it |
| `pkce.rs` | proof key for code exchange (rfc 7636) |
| `tokens.rs` | where the google sign-in is kept between runs |
| `upload.rs` | uploading one file to google drive |
