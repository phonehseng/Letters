# organise worklog

## 2026-11-12 move, copy, rename, recycle, undo
File organising for Recents and the library. Nothing is replaced silently:
a name clash returns `Exists` so the interface can offer Keep Both, Replace
or Skip, and Replace sends the old file to the recycle bin rather than
deleting it. A move to another drive copies, compares the bytes and only
then removes the original. Recycling uses the system's own recycle bin
through powershell (windows) or the finder (mac), so no new library was
added. Every move, copy and rename returns a `Done` that `undo` reverses,
never over a file that has appeared in the old place since.
