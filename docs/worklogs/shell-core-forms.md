# shell-core forms worklog

`app/shell-core/src/forms/`: the Fill & Sign glue that needs no window.

## 2026-11-12: every save of a signed document is an incremental update

A save used to copy the working copy, which lopdf had written out whole,
so changing a signed document and saving it broke the signature; Save was
refused instead and the person had to save a copy. Now the edit session
stores every change to a signed document as an incremental update of the
version before it (`signed::incremental_version`, core/forms'
`save_incremental`): the signed bytes stay exactly where they were and the
change is appended. Save may then write it over the signed file, and the
first signature still checks out, listing the change as made after it.

Save checks the rule directly rather than trusting a flag: the version must
begin with every byte of the file it replaces (`file_starts_with`). An undo
back past the signature, or a clean rewrite after a redaction (Protect's
writer), doesn't, so those saves are still refused with the Save As
sentence. A clean rewrite is refused on purpose: a redaction appended after
a signature would leave the removed text in the earlier revision.

A file opened already signed is guarded from its first edit: the session
looks for `/ByteRange`, which every signature dictionary has, when it
starts. Before, the guard went on only when Fill & Sign had read the
signatures, so an edit made in the first moments could still be saved as
a rewrite. A stray `/ByteRange` in an unsigned file only makes its saves
incremental, which is harmless.

Save a Copy no longer drops the guard: the copy is still signed, so its
changes are appended too. The command that dropped it is gone.
