# Worklog: app/ui/src/lib/updates

## 2026-11-12 in-app updates

Letters updates itself from GitHub releases through tauri-plugin-updater
(app/shell/src/updates.rs; the once-a-day schedule and the short notes are
in app/shell-core/src/updates.rs). The installer is signed with the release
key and refused unless the signature matches the public key in
tauri.conf.json. The first window checks quietly four seconds after it
opens, at most once a day, never while its document has unsaved changes,
and only while Settings › General allows it; Check for Updates… asks at
any time. `UpdateNote.svelte` shows what changed in a few plain lines,
with Update and Restart or Later.

Update and Restart downloads and checks the update first, then closes
every window the way it always closes, so unsaved changes are asked
about. Cancel in that question leaves the update waiting, with a message
saying so. The update is installed only once the last window has gone,
on the way out, and the new version starts by itself. Installing never
touches the app's data and cache folders, so settings, Recents and saved
versions stay.

Tried on Windows with a signed 0.1.0 installed and a test 0.1.1 from a
prerelease (LETTERS_UPDATE_ENDPOINT points the check at its latest.json):
the quiet note appeared, the save question came up for a changed text
file, Cancel kept 0.1.0 running with the waiting message, Check for
Updates… found it again, Save then let the installer run, and 0.1.1
started with the same document open. Hashes of the data folders before
and after matched apart from Recents' order, the search index and the
system font list, which every launch rewrites, and the new
update-check.txt.
