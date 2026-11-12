# Worklog: app/shell-core/src/associations

## 2026-11-11 registered file types and where they open

Added the list of types the installer registers and `route_for`, with a test
that compares the list against `tauri.conf.json`'s fileAssociations so the two
can't drift. `file_arguments` reads every file on the command line, because
Explorer passes several when they are opened together.

## 2026-11-12 rtf and doc go to the writer

The writer now converts .rtf and .doc itself, so they route there instead of
being turned into pdfs.

## 2026-11-12 installer built and inspected

Built `Letters_0.1.0_x64-setup.exe` (42.7 MB) and read the generated NSIS
script instead of running it: per-user install to `%LOCALAPPDATA%\Letters`,
`RequestExecutionLevel user`, one `APP_ASSOCIATE` per extension with the
`Letters.<ext>` classes, our hook adding document icons and the Default Apps
capabilities under HKEY_CURRENT_USER, and `APP_UNASSOCIATE` plus the hook's
removal on uninstall. Real-app proof: four files on the command line opened
four windows on the right screens, and a second start handed its file to the
running copy.
