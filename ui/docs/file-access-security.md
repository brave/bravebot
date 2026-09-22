# UI file access and retention

Project text previews, attachment validation, and bot-memory reads, edits and seeding use
`bravebot-ui-files`, a helper built and packaged by this repository. It is independent of
the agent crates. Only Electron's main process supplies its project root and path;
the renderer cannot choose a helper executable or send a shell command.

The helper opens each directory component with `openat`, `O_DIRECTORY`, and `O_NOFOLLOW`,
starting from the filesystem root. Reads use the pinned parent descriptor. Memory edits
create an exclusive temporary file and use `renameat` within the pinned parent directory.
Replacing a path component with a symlink therefore cannot redirect a later operation.
An operation already holding a directory descriptor stays on that authorized directory
even if its name changes. Symlinked project paths are refused, except for macOS's fixed
`/tmp` and `/var` aliases. Read sizes, request sizes, helper runtime and output are bounded.

The replacement helper accepts only `.bravebot-ui/bots/*.md`, checks the expected previous
text, and writes private regular files. This protects the outside-file boundary; it does
not claim to lock out another editor that concurrently changes the same authorized file.

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
leaf symlinks, links at the memory path and at the briefing path, traversal, size bounds,
stale edits, file permissions, removal and bot recreation, and actual IPC in a packaged app.
It also covers what a briefing may carry: that it names the memory file and quotes no byte
of it, and that a second grounding leaves an existing memory alone. See [testing.md](testing.md) for reproduction commands.
