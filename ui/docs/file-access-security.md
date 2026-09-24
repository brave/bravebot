# UI file access and retention

Project text previews, attachment validation, and bot-memory reads, edits and seeding use
`bravebot-ui-files`, a helper built and packaged by this repository. It is independent of
the agent crates. Only Electron's main process supplies its project root and path;
the renderer cannot choose a helper executable or send a shell command.

On POSIX the helper opens each directory component with `openat`, `O_DIRECTORY`, and `O_NOFOLLOW`,
starting from the filesystem root. Reads use the pinned parent descriptor. Memory edits
create an exclusive temporary file and use `renameat` within the pinned parent directory.
Replacing a path component with a symlink therefore cannot redirect a later operation.
An operation already holding a directory descriptor stays on that authorized directory
even if its name changes. Symlinked project paths are refused, except for macOS's fixed
`/tmp` and `/var` aliases. Read sizes, request sizes, helper runtime and output are bounded.

On Windows the same walk is `NtCreateFile` relative to the parent's handle, since Win32 has no
relative open. `OBJ_DONT_REPARSE` and `FILE_OPEN_REPARSE_POINT` stop it following a symlink or a
junction at any component, and one opened as itself is refused, as `O_NOFOLLOW` refuses a symlink.
The edit's rename and the temporary file's removal act on the open handle within the pinned
parent. The walk starts at a drive letter's root: a UNC share, a `\\?\` path and a device path are
refused rather than walked. A reparse point that is not a link, such as a cloud-files placeholder,
is opened as itself, since it names no other file.

Before any of that, a request has to name a path inside a project at all, and that is decided in
two places. `shared/files.ts` holds what is true on every platform: a relative path with no empty,
`.` or `..` segment, reading a backslash as a separator, since Windows reads `a\..\..\x` as two
climbs out of the project and a POSIX host reads a backslash as an ordinary character either way.
That module is shared with the renderer, which is given no `process`, so it cannot ask which
platform it is on. `main/files.ts` holds what is a refusal only on Windows, where it can ask:
`x:stream` naming an alternate data stream, `NUL` and its siblings naming a device, and a trailing
dot or space that Win32 drops before it looks. None of those is a way out of the project, which
the `realpath` check decides; each is a request resolving to something other than what it spells.
Applying them everywhere would cost a POSIX project every file it has with a colon in its name.

The helper refuses the same forms again on Windows, per component and before it checks what path
was asked for, along with a reserved character and an 8.3 short name, which is another file's
second spelling. A short name is accepted in the project root, the directory the user chose, since
the system temporary directory is often spelt that way. NTFS matches names without regard to case,
so `HOOKS.JSON` opens `hooks.json`; the helper compares a path exactly, and any other case is
refused rather than normalised.

The replacement helper accepts only `.bravebot-ui/bots/*.md`, checks the expected previous
text, and writes private regular files. This protects the outside-file boundary; it does
not claim to lock out another editor that concurrently changes the same authorized file.
On Windows, private means an explicit, protected access list set when the helper creates the file
or directory, granting the account it runs as full control and no one else anything: what 0600
and 0700 are on POSIX. It is not the list inherited from the profile. A directory that was already
there keeps the list it has, as it keeps its mode on POSIX.

Grounding a bot turn goes through the same walk, on a `memory.seed` operation that accepts
the same path shape. It makes `.bravebot-ui`, `.bravebot-ui/bots` and the memory file exist
and answers only whether it created one: there is no operation here that returns memory text
to the grounding path, because what the memory says is the model's own writing and a briefing
the app vouches for may hold only bytes the app wrote. A seed writes only when the file is
absent, and never reads or judges what is in one that is there. It refuses a link, a
directory or anything else that is not a regular file rather than replacing it, since the
path is the app's own and something else at it was arranged. The `.gitignore` beside it is
written only when absent, and a link or a directory in its place is left alone rather than
failing the seed: nothing a memory read depends on is in that file.

The briefing itself is written under the app's own data directory rather than a checkout, so
it is not pinned to a project root. It is written to a name of its own and renamed into
place, never opened by the name the turn will name, so a link left at the briefing's path is
displaced instead of written through.
Previews never send content to a model. Attachment selection still requires the native
picker and a separate Send action.

Drafts are stored locally in `experience.json` with mode 0600; clearing a draft removes
its saved text. Memory history retains up to 30 revisions. Reset preserves revisions for
recovery, as stated in its confirmation. Deleting a bot removes its app-owned revision
history and cached briefing, but keeps project memory files and saved conversations.
Deletion errors are surfaced, and deletion is refused while a bot's conversation is running.
These are local files, not encrypted storage or a promise of secure erasure from backups.

Regression coverage includes parent-directory swaps for reads, writes and seeds, root and
leaf symlinks, a link at every component (a junction as well on Windows, where CI runs the
helper's suite), Windows name forms, links at the memory path and at the briefing path,
traversal, size bounds, stale edits, file permissions, removal and bot recreation, and actual
IPC in a packaged app.
It also covers what a briefing may carry: that it names the memory file and quotes no byte
of it, and that a second grounding leaves an existing memory alone. See [testing.md](testing.md) for reproduction commands.
