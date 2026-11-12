# Worklog: app/shell-core/src/settings_store

## 2026-11-11 the settings window's stored choices

Added `app-settings.json` (General, Files, Scanning, AI Tools, the model
server's files) with a default for every field, so a missing or damaged file
reads as the defaults. Unknown fields are kept so an older Letters never drops
what a newer one wrote. Settings that already had a home (name on comments,
typing, Google Drive) stay there, so nothing is stored twice. A test checks
that a key saved in the key store never reaches the file.

## 2026-11-12: the time stamp server

Settings gained a `signing` section with one field, `timestampServer`,
empty by default. Empty means no time stamp is ever requested: signatures
use the signer's clock and say so. Only Sign's Add a Trusted Time reads it.
