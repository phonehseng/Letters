# Worklog: Google Drive (app/shell-core/src/drive)

## 2026-11-12 formatting only

Ran rustfmt on the folder; no behaviour changed. The client id is now entered
in the Settings window (Settings › Accounts), so the writer's Drive sheet no
longer has its own field; this folder reads it from the writer settings as
before.

## 2026-11-12 a README for the folder

Listed the folder's files, one line each. fake_google.rs's serve function is
longer than the guideline (120 lines, one match arm per Google endpoint); left
as it is, since splitting it would only scatter the endpoints.
