# UI file access and retention

Project text previews, attachment validation, and bot-memory reads and edits use
`bravebot-ui-files`, a helper built and packaged by this repository. It is independent of
the brave-bot submodule. Only Electron's main process supplies its project root and path;
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
Previews never send content to a model. Attachment selection still requires the native
picker and a separate Send action.

Drafts are stored locally in `experience.json` with mode 0600; clearing a draft removes
its saved text. Memory history retains up to 30 revisions. Reset preserves revisions for
recovery, as stated in its confirmation. Deleting a bot removes its app-owned revision
history and cached briefing, but keeps project memory files and saved conversations.
Deletion errors are surfaced, and deletion is refused while a bot's conversation is running.
These are local files, not encrypted storage or a promise of secure erasure from backups.

Regression coverage includes parent-directory swaps for reads and writes, root and leaf
symlinks, traversal, size bounds, stale edits, file permissions, removal and bot recreation,
and actual IPC in a packaged app. See [testing.md](testing.md) for reproduction commands.
