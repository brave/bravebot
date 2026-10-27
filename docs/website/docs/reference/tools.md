---
sidebar_position: 3
title: Tools
description: Every tool the model may call, what it takes, and what it is allowed to touch.
---

# Tools

There are twenty-six tools, and no way to add another from a configuration file. Each one splits its
arguments into **routing**, the part that decides where the effect lands, and **content**, the part
that is merely carried.

| Tool | Routing | Content | Asks you? |
|---|---|---|---|
| [`read_file`](#read_file) | `path`, `path_ref`, `offset`, `limit` | none | only to trust a quarantined file |
| [`list_files`](#list_files) | `directory`, `pattern`, `depth` | none | no |
| [`search`](#search) | `pattern`, `directory`, `include`, `offset`, `case_sensitive`, `context`, `output` | none | no |
| [`repo_map`](#repo_map) | `directory`, `budget` | none | no |
| [`read_git`](#read_git) | `query`, `repository`, `revision`, `path`, `pattern`, `count`, `skip`, `messages`, `since`, `until` | none | only if what it would show holds a credential |
| [`lsp`](#lsp) | `operation`, `path`, `line`, `character`, `query` | none | **yes, to start a language server** |
| [`write_file`](#write_file) | `path`, `path_ref`, `contents_ref` | `contents` | **yes, every time** |
| [`edit_file`](#edit_file) | `path`, `path_ref`, `replace_all`, `cell`, `cell_action`, `cell_type` | `old_text`, `new_text`, `edits` | **yes, every time** |
| [`apply_checkout`](#apply_checkout) | `checkout`, `paths` | none | **yes, for every file** |
| [`run`](#run) | the compiled plan, `directory`, `background`, `deadline_seconds`, `stdin_ref`, `read`, `scopes` | stdin | **yes, unless vouched for, remembered, ruled on or proven** |
| [`read_output`](#read_output) | `ref`, `offset`, `pattern` | none | **yes, unless the planner may already read it** |
| [`vet_content`](#vet_content) | `ref` | none | **yes, that is what it is for** |
| [`job_output`](#job_output) | `job`, `kill`, `wait_seconds`, `until_exit` | none | no |
| [`fetch_url`](#fetch_url) | `url` | none | **yes, unless a rule names the host** |
| [`download_url`](#download_url) | `url`, `path` | none | **yes: the host unless a rule names it, and the destination every time** |
| [`spawn_processor`](#spawn_processor) | `reads`, `about` | `instruction` | no |
| [`spawn_agent`](#spawn_agent) | `kind` | `task`, `each` | not the call, but its writes and runs do |
| [`load_skill`](#load_skill) | `name` | none | no |
| [`search_skills`](#search_skills) | `query` | none | no |
| [`ask_user`](#ask_user) | the questions | none | it *is* the question |
| [`propose_plan`](#propose_plan) | the plan | none | it *is* the plan you approve |
| [`request_path`](#request_path) | `path`, `write` | none | **yes, unless you bypass permissions** |
| [`todo_write`](#todo_write) | none | `todos` | no |
| [`remember`](#remember) | `key`, `scope`, `supersedes`, `new` | `text` | no |
| [`recall`](#recall) | `query`, `key` | none | no |
| [`propose_rule`](#propose_rule) | `scope` | `text`, `reason` | **yes, that is what it is for** |

Five more are offered only where they mean something.
[`request_path`](#request_path) goes wherever `run` is confined, so not to a delegate.
[`schedule_next`](#schedule_next) goes to a turn that will be asked again: one inside a self-paced
[`/loop`](commands.md#loop-interval-prompt), and one on a line you typed in a session that can send
it again. Not to a tick of a loop you gave an interval for, not to a delegate, and not where nothing
will ask again, which is a one-shot run, the desktop application, or a line the agent wrote itself.
[`watch_file`](#watch_file) goes to a session that keeps watches, so not to a delegate, a one-shot
run or a planned run. [`advisor`](#advisor) goes to a session that named a model to consult with
[`--advisor`](cli.md#--advisor-name), the [`advisorModel`](../customize/configuration.md#advisormodel)
setting or [`/advisor`](commands.md#advisor-model--off), so not to one that did not, and not to a delegate.
[`message_session`](#message_session) goes to a background session, so not to a session in a terminal, a
delegate or a one-shot run.

[`remember`](#remember), [`recall`](#recall) and [`propose_rule`](#propose_rule) are the planner's side of [memory](../customize/memory.md). None of them
is offered to a delegate, and a session that writes nothing under `~/.bravebot`, an
[incognito](../using/sessions.md#a-session-that-leaves-nothing-behind) one, saves, verifies and proposes nothing.

Every tool also takes `why`, one line from the planner saying what the call is for. It is content
on every tool: it is drawn beside the call for you to read, and nothing reads it or decides on it.
Every tool lists it as required, and a call that leaves it out still runs, drawn with no reason. A
server's tool is the exception, since it is offered as the server
describes it.

A number or a flag that shapes a call is routing too, not content: nothing carries it anywhere, so it
sits on the same footing as the fields beside it. A routing argument naming a **reference** rather
than a path is bound to the context the planner named it in, so a turn whose context has met
untrusted content can name no reference at all.

An unknown tool is reported to the planner rather than ignored.

When the planner asks for several calls in one reply, they run in the order it asked, except that two
or more consecutive [`list_files`](#list_files) and [`search`](#search) calls are read at the same
time, eight at most. Their results are added in the order the calls were asked, and what is
quarantined, what the [audit trail](../security/audit-trail.md) records and what is drawn are what
they would be if the calls had run one after another. Every other call, [`read_file`](#read_file)
included, runs alone, after the calls before it and before the calls after it. So does a call that
has a [`tool-finished` hook](../customize/hooks.md) declared for its tool, since the hook could
change the files the next read looks at.

There is **no shell tool**, and there never will be. Nothing the planner writes is handed to an
interpreter. [`run`](#run) takes a command line, and bravebot compiles it itself. See
[Shell mode](../using/shell-mode.md).

---

## `read_file`

Reads a text file from the workspace and returns its lines.

| Parameter | |
|---|---|
| `path` | workspace-relative path |
| `path_ref` | a reference to a file whose name the planner was not shown, e.g. `ref:2` |
| `offset` | 1-based line to start at |
| `limit` | maximum lines to return, capped so one read cannot fill the conversation |
| `output_of` | in a `.ipynb` notebook, the number of a code cell, from 1 as a read shows them, to get that cell's saved outputs as a reference instead of the cells |

A `.ipynb` notebook that the agent may read comes back as its cells instead of its JSON: a
`[N] kind` line, numbered from 1, then the cell's source. Saved outputs are never shown, only how many
cells hold them, because an output is text a program or a web page produced. A notebook you have not
vouched for is a reference like any other file.

`output_of` asks for one cell's saved outputs. They come back as a reference, even in a notebook you
vouched for, and a processor reads them. A stream, an error and a result's plain text are kept, a
picture or other data is replaced by a line giving its type and size, and the result is cut at
20,000 characters.

Long files come back one page at a time, at most 500 lines and 100,000 characters, with each line
shortened to 2,000 characters. A page of long lines ends at the last whole line that fits under the
character cap. The result says so and gives the offset to continue from. A
file that is not text is reported as binary, a picture being the exception. A UTF-16 or UTF-32
file that begins with a byte order mark is shown decoded, and so is a file with a few bytes that
are not UTF-8, such as Latin-1 text, with each such byte shown as U+FFFD. The result says when it
decoded the file, and `edit_file` refuses a file that is not UTF-8, since it would write the file
back as UTF-8.

**A picture is quarantined whatever the trust map says, and leaves quarantine only through
[`vet_content`](#vet_content).** A file whose extension names a picture or a PDF comes back as a
reference saying what kind of thing it is, and vouching for the directory does not change that: what
the trust map answers is whether a file's *text* may be read, and a picture has none. Handed to
[`spawn_processor`](#spawn_processor) it arrives as a picture rather than as base64, so the model
looks at it, and what the processor says back is quarantined like any other processor's answer.
The planner sees the picture itself only if it asks for that one with `vet_content` and it is let
through.

The reason is that a screenshot carries whatever words are in it, so a picture reaches the planner's
context only if you put it there (a picture you dropped on the terminal) or a check looked at it and
you let it through. A path in model output is neither. What kind of file it is comes from the extension and never from the
bytes, so a file cannot become a picture by holding something that looks like one, and a picture
cannot become text by being called `.txt`.

**A read the planner may not see does not open the file.** Where the content would be quarantined,
you are offered the chance to vouch for that one file at the moment it matters, unless the path names
a picture, since a yes there would grant nothing. An empty file, or one that turns out not to be text,
is asked about like any other. See
[the quarantined-read prompt](../security/trust.md#the-quarantined-read-prompt).

### Every read carries a change token

A read the planner may see comes back with a short opaque token. The same token on a later read means
nobody wrote the file in between; a different one means somebody did. It covers the whole file rather
than the window returned, so asking for less of a file is not mistaken for the file changing, and there
is no hour to be read out of it.

**It says the file was written, not what changed.** A rewrite restoring the same bytes moves the
token, and a filesystem that leaves the modification time alone moves nothing. What it answers is
whether the file looks written to since the last look.

This is how "tell me when this changes" is answered where no watch can be armed: the file is read now,
and the turn schedules the look that would catch a change. In a session that keeps watches,
[`watch_file`](#watch_file) is the better answer and this tool says so. For a file the planner may not
be shown there is no token, and the size in the reference is what there is to compare.

### A repeat read of a file nobody wrote is answered with a notice

Ask for the same window of a file again, with the file as it was, and the result is a short notice
that repeats the change token instead of the lines, which are already earlier in the conversation. A
different offset or limit, or a file written since, comes back as lines. After the conversation is
compacted the lines are sent again, because the round that held them is no longer sent.

## `list_files`

Lists files under a directory.

| Parameter | |
|---|---|
| `directory` | workspace-relative; `.` for the root |
| `pattern` | optional glob: `*`, `?`, `**` and brace groups like `**/*.{rs,toml}`. One with a `/` may be written from `directory` or from the workspace root |
| `depth` | optional; how many directory levels below `directory` to walk, `1` being that directory and no further |

Set a `depth`. Without one the walk reaches every file underneath, which in a real repository is
thousands of paths. You pay for them in the planner's context, again on every round that resends it,
and again in each delegate handed the same question.

A **bounded** listing names the directories it did not descend into alongside the files, so what
comes back describes the shape of the tree. The pattern does not hide those directories: it says
which files are wanted, and a directory is where the answer might be rather than an answer.

A listing of a directory nobody vouched for is quarantined, because a filename is content. It returns
**one reference per entry**, not one for the listing. That is what lets the planner read a file,
process it and write it back without ever being told what it is called. The directories a bounded
walk stopped at are entries too, and each reference says which of the two it stands for: there is
nothing behind a directory to read, and the way to what is inside it is another listing with a
greater depth.

A walk does not enter a fixed list of directories, at any depth, and its result does not say that it
left them out: version control (`.git`, `.hg`, `.svn`), build output and caches (`target`, `dist`,
`build`, `coverage`, `__pycache__` and similar), dependencies (`node_modules`, `vendor`,
`third_party`, `site-packages`, virtual environments and similar) and linked worktrees (`.worktrees`,
`.claude/worktrees`). A project with a `vendor` or `build` directory therefore lists as though it had
none. The tool description gives the planner the full list, and naming one of them as `directory`
lists it.

A walk does not enter a symbolic link, to a file or to a directory, and does not name one. Its
result says how many links it did not enter, as a number, and a directory made only of links is
said not to be empty. That holds when the names are quarantined, where the count comes beside the
references. To see what is behind a link, list the directory it points to by its own name.

The glob is literal and the matcher does not backtrack. A truncated listing says it was truncated.

## `repo_map`

Lists the declarations in a directory's source files, the ones other files mention most first. It
needs no language server, so it answers where [`lsp`](#lsp) cannot be started.

| Parameter | |
|---|---|
| `directory` | workspace-relative, defaults to `.` |
| `budget` | optional; tokens the map may take, 100 to 8000, 1000 unless given |

It reads Rust, Python, JavaScript, TypeScript, Go, Java, Kotlin, C and C++ files, and recognises a
declaration by the line that opens it, so a signature split over lines is shown by its first line. In
Java, C and C++ it finds types and macros, not functions or methods. A name is ranked by how many
other files mention it, so the ranking is a guide, not a call graph.

Only files you vouched for are read. Files you did not are counted and never named, a file holding
what looks like a credential contributes nothing, and a file a `deny` rule covers is not mapped. A map
that shows fewer declarations than it found, or stopped at a cap, says so: map a subdirectory, or raise
`budget`.

## `search`

Finds lines matching a **regular expression** in workspace files.

| Parameter | |
|---|---|
| `pattern` | a regular expression. May be a list, in which case a line matches if it matches any of them |
| `directory` | workspace-relative, defaults to `.`. May name one file, which is then the only file searched and `include` is not consulted |
| `include` | optional glob limiting which files are searched: `*`, `?`, `**` and brace groups like `**/*.{cc,h,mm}`. One with a `/` may be written from `directory` or from the workspace root |
| `offset` | which match to resume from, to read past the match cap ([below](#a-capped-search-can-be-asked-past-its-cap)) |
| `case_sensitive` | defaults to true. `(?i)` in the pattern asks for the same thing |
| `context` | lines to show before and after each match, as `grep -C` does: none unless given, at most 10 |
| `output` | `lines` (the default), `files` for only the paths of the files with a match, or `count` for the matching lines per file and in all. The last two ignore `offset` and `context` and are not held to the match cap ([below](#files-and-counts)) |

Supported: literals, `.`, `*`, `+`, `?`, `|`, `(...)`, `(?:...)`, `[...]` with ranges and negation,
`\d`, `\w`, `\s` and their negations, `^`, `$`, `\b`, `\B`, and a backslash before a metacharacter to
match it literally. `(?i)` ignores case from where it is written to the end of its group and `(?-i)`
stops ignoring it, while `(?i:...)` and `(?-i:...)` apply to only what they enclose.

Two things are absent. **Counted repetition** (`a{2,9}`) is not supported and `{` is an ordinary
character. **Backreferences** are not supported. Captures are never extracted, since a search reports
the whole line. Lookaround, named groups and flags other than `i` are not supported either, and a
pattern using one is refused with a message saying so.

The engine does not backtrack, so a pattern like `(a+)+$` that is exponential elsewhere costs nothing
unusual here: matching is the line's length times the pattern's size, whatever the pattern. That bound
is why the two missing constructs are missing. Counted repetition nested twice multiplies the states a
short pattern expands to, and backreferences cannot be matched without backtracking.

A list of patterns is alternation, one more expression to try per line, so the work is the sum of the
patterns rather than a power of anything. A pattern that is too long or nested too deeply is refused.
Brace groups in `include` are expanded before the walk. Case is folded by the engine rather than by
lowercasing the pattern, which would turn `\D`, `\W` and `\S` into the classes they negate, and a line
is reported as written rather than as it was folded to match.

**Version control, build output, caches and vendored dependencies are not walked**, by name and
without reading anything to decide it. The project's own `.gitignore` is not consulted either: it
would decide what to walk from a file in the tree being walked, and a tree that can hide files from a
search can hide them from review.

A search does not enter a symbolic link, to a file or to a directory, so a file behind one is not
read. Every result says how many links were not entered and that the files behind them were not
searched, as a number with no name, whether or not the search found matches. To search behind a
link, name the directory it points to as `directory`.

A result touching several files is trusted only if **every one of them** is. A truncated search tells
the planner it is incomplete. A search that found nothing says which kind of nothing it is: no
matching lines in the files it read, or an `include` that selected no files at all. Those are
opposite facts, and drawn identically the planner reads one as the other. A glob leaning on syntax the
matcher does not have is named for the same reason.

There are three ways to come back partial, and all three say so: stopping at the match cap, stopping
before every file was opened, and running out of time. The last two are the dangerous ones, because
with nothing found there is nothing that looks incomplete. Which files a capped search kept does not
depend on the order the filesystem handed them over: each directory is sorted and its own files are
taken before descending, so a partial answer is the shallow part of the tree and is the same answer
on every machine.

### A capped search can be asked past its cap

A search stopped by the **match** cap reports the offset of the first match it left behind, and asking
again with that `offset` returns the matches from there. An offset past the last match returns nothing
and says how many matches there were.

Only the match cap can be asked past. A walk that stopped short of the tree or ran out of time is not
a page to be continued: narrow the pattern or point at a subdirectory instead.

### Lines around a match

With `context`, each match comes with the lines either side of it, written `path-line- text` where a
match is `path:line: text`, and a `--` line between groups that are not adjacent. Those lines are not
matches: they count toward neither the match cap nor the offset. A separate cap bounds how many are
returned, and a search that reached it says it is incomplete.

### Files and counts

`output: files` lists each file holding a match once and no line; `output: count` gives how many
lines match in each file and in all. Neither is held to the match cap, so a count totals every
match the walk read. A walk that stopped at the file cap or the time cap says the result is a lower
bound, and so does a list cut at its own cap of 200 files.

### The caps are configurable

`search.maxFiles` and `search.maxSeconds` set how many files a search may walk and how long it may
spend opening them. Either can be raised as well as lowered, and the two are independent. The built-in
caps are past what an ordinary repository holds; a monorepo, a tree of generated sources, or a
checkout on a network filesystem is where they are not, and there every search comes back partial.
See [Configuration](../customize/configuration.md).

Raising a cap does not unbound a search. The walk still stops at `search.maxFiles`, the reading still
stops at `search.maxSeconds`, and the match cap holds regardless of both.

## `read_git`

Reads a repository's history from the files under its `.git`, and its status from those and the
working tree, **without starting git**.

| Parameter | |
|---|---|
| `query` | `log`, `show`, `diff`, `status`, `tags` or `search` |
| `repository` | workspace-relative directory holding `.git`, defaults to `.` |
| `revision` | in git's syntax: a branch, a tag, `HEAD`, an id or its prefix, then `~N`, `^N` or `^{commit}`. `log` takes one or a range `A..B`, or `A...B` for what either side has that the other lacks; `show` takes one, or `<revision>:<path>` for a file or directory as it was; `diff` takes two, as `A..B` or `A B`, or `A...B` to compare the merge base with B; `tags` takes none, or one to list only the tags it reaches; `search` takes one and defaults to `HEAD` |
| `path` | relative to the repository's root. Limits `log` to commits that changed it, `show`, `diff` and `status` to changes under it, and `search` to files under it |
| `pattern` | `search` only, and required there: the regular expression to look for, matched against each line |
| `count` | commits a `log` lists, tags a `tags` lists or lines a `search` prints: 20 unless given, at most 200 |
| `skip` | `log`, `tags` and `search`: how many to pass over before listing, as `git log --skip` does |
| `messages` | `log` only: print each commit's whole message beneath its line |
| `since`, `until` | `log` only: whole days in UTC, written `YYYY-MM-DD`, both ends included |

`log` prints one commit per line: the first ten characters of its id, the day it was authored, its
author and its subject. With `messages`, the rest of each commit's message follows its line,
indented. A log that stopped with commits left names the `skip` that lists the next page, and a page
holds whole commits. `show` prints a commit
with its message and diff, a tag with its message and then its commit, or a file or directory at a
revision. `diff` compares two commits. Two edits far apart in a file show as two hunks. Where a stretch of a file cannot be lined up, the answer says which line ranges differ and gives the lines removed and added as upper limits. A merge is shown without a diff and says which diff to ask for.
`tags` lists tags newest version first, as `git tag --sort=-v:refname` does, and given a revision
only those it reaches, as `--merged` does. `search` prints the lines `pattern` matches in the files
at a revision, as `git grep -n` does. Both page with `skip` as a log does.
`status` lists staged, unstaged and untracked paths as `git status --short --no-renames` does. It
detects no renames, and lists a file an attribute or `core.autocrlf` would convert, or a
submodule, as not compared.

**Why not just run git.** git runs programs its configuration names: an alias, a pager, a diff
driver, `core.fsmonitor`, and `include.path` pulls configuration in from any file. That configuration
lives in the repository being inspected, so [`run`](#run) cannot prove a git command safe and asks
unless something you set already covers it. `read_git`
applies nothing the configuration names and returns no remote URL, so the same question needs nobody
to answer it.

**It opens only a repository you trust in full.** Following history means following what the files
under `.git` say, so `.git` and everything beneath it has to be trusted before any of it is read.
Anywhere else it says so, and the planner uses `run`. `status` also reads every file in the working
tree, so it answers only where you trust the whole of it; log, show and diff still answer where you
distrust part of it. An answer showing a file you distrust is
quarantined like a read of that file, since a commit holds that file's bytes, and so is a search
that read one, whether or not anything in it matched.

**A deny rule on a file covers its history.** Naming a denied file, as a `path` or as
`HEAD:.env`, is refused. A denied file met in a diff, a listing or a search is left out, and the answer says
so. A rule over `.git` or anything in it keeps the repository closed.

What it would show is scanned for credentials as a file read is, and held back until you agree. A
file's lines in a commit are scanned as that file, so agreeing to one file's key is not agreeing to
another's. A `run` of git whose output the planner could not be shown mentions `read_git`, unless `read_git` would
decline the repository the command ran in.

Blame, `--follow` and a diff against the working tree go through `run`. A repository laid out in a
way that changes what a read means, such as borrowed objects, replace refs, an included
configuration file, `core.worktree`, or a `.git` that is a file, is declined with the same pointer.
So is a status over a split or sparse index, a bare repository, or ignore and attributes files
named outside the repository. An answer cut by the count, by 2,000 lines, or by the search
deadline says it was cut.

## `lsp`

Asks a language server about a symbol: where it is defined, what refers to it, what implements it,
what calls it. It also asks what the server makes of a file. **Starting a server is put to you.**
That holds in every permission mode except [bypass](../security/permission-modes.md) with the sandbox
mode `off`, which answers it for you. A start is refused where nobody can be asked, as in a one-shot
run, unless that answers it. The full reference is [the lsp tool](lsp-tool.md).

| Parameter | |
|---|---|
| `operation` | one of `goToDefinition`, `findReferences`, `hover`, `documentSymbol`, `workspaceSymbol`, `goToImplementation`, `incomingCalls`, `outgoingCalls`, `diagnostics` ([details](lsp-tool.md#checking-a-file-with-diagnostics)) |
| `path` | workspace-relative file holding the symbol, or the file to check for `diagnostics`; required for every operation but `workspaceSymbol` |
| `line`, `character` | 1-based position of the symbol, as a search or a read reported it; not used by `diagnostics` |
| `query` | for `workspaceSymbol`, the name to look for |

This answers what [`search`](#search) cannot. A search for a name finds every comment and string that
mentions it; this finds the declaration the compiler agrees on.

Which server runs and what starting one costs, how to
[declare a server of your own](lsp-tool.md#declaring-a-server-of-your-own), how a location and the
text at it are labelled, and where the index is cached are in [the lsp tool](lsp-tool.md).

## `write_file`

Writes a UTF-8 text file in the workspace. **You approve every write before it happens.**

| Parameter | |
|---|---|
| `path` | workspace-relative destination |
| `path_ref` | a reference to the file to write, for a file the planner was never shown the name of |
| `contents` | the complete new contents |
| `contents_ref` | a reference whose quarantined content becomes the whole file |

Contents **or** a reference, never both. A reference that names no file is not a destination. The
planner never chooses a destination on its own, and a write through a `path_ref` is always shown,
since you are the only one who sees which file it is. A path outside the working directory and the
directories you opened with [`/add-dir`](commands.md) is refused before you are asked, and the
refusal says how to open the directory, since no answer to the question would make the write
possible.

See [Trusted directories](../security/trust.md#what-a-write-does) for what a write does to the trust
map.

In the terminal, the prompt for a write whose `contents` the agent wrote offers `e`, which opens them in
your editor and puts the write to you again as a diff of the text you saved
([details](../security/permissions.md#changing-a-write-before-approving-it)). What you saved is
scanned for credentials, and the agent is told that you changed the contents and not what they say. A
write from a `contents_ref` does not offer it.

Replacing a **trusted** file with `contents` keeps that file's line endings and its leading byte
order mark, so a file using `\r\n` stays CRLF and a file that began with a mark still does. This
happens before the diff you approve is built, so the approval shows the lines the write changes
rather than every line in the file, and it says `line endings: CRLF kept`. A file nobody vouched for
is not converted, because its terminators are untrusted bytes and nothing may be decided from
those. A file written from a `contents_ref`, and a path with no file yet, are written as given.

Where a language server for the file's language is **already running**, the result also says how many
errors it found and on which lines, and how many warnings. A write never starts a server, and the
server's own messages are never shown, only counts and line numbers. A server that has not finished
indexing, or that did not answer in a few seconds, is reported as such rather than as "no errors". A
file written from a `contents_ref` reports nothing.

## `edit_file`

Replaces an exact passage in an existing file. **You approve every edit, as a diff.** The agent
prefers this to rewriting a whole body.

| Parameter | |
|---|---|
| `path` / `path_ref` | the file |
| `old_text` | the exact text to replace, matched byte for byte (a file whose lines all end in `\r\n` is the one exception, below) |
| `new_text` | what goes in its place |
| `replace_all` | replace every occurrence instead of requiring exactly one |
| `edits` | instead of `old_text` and `new_text`: several pairs of them for this one file |
| `cell` | in a `.ipynb` notebook, the number of the cell to change, from 1 as a read shows them, with `new_text` as its whole source; 0 with `insert_after` puts a cell first |
| `cell_action` | `replace` (the default), `insert_after` or `delete` |
| `cell_type` | the kind of cell `insert_after` makes, `code` (the default) or `markdown` |

`edits` is applied in order, each pair to the text the one before it left, and all or nothing: if
one pair fails, the agent is told which and the file is not changed. You are asked once, about one
diff of the combined result.

The result gives the agent back each changed region with three lines either side, fewer where many
regions would make the result long, and says how many unchanged lines lie between two regions. Those
lines are scanned for credentials as a file read is. If they hold what looks like a credential and
you have not agreed to the agent being shown that file, you are asked, and one answer covers the file
for edits and reads alike. A no leaves the edit in place and the lines out of the result.

Where a language server for the file's language is already running, the result also says how many
errors it found and on which lines, and how many warnings, as a `write_file` result does
([details](lsp-tool.md#checking-a-file-with-diagnostics)). An edit never starts a server, and the server's own
messages are never shown.

`old_text` must occur exactly once unless `replace_all` is set. An edit **refuses rather than
guesses**.

In a file whose every line ends in `\r\n`, the agent's `\n` is read as `\r\n`, both in the passage it
looks for and in what it puts there, so the file keeps one terminator throughout. The approval says
`line endings: CRLF kept`. A file that mixes `\n` and `\r\n` is matched exactly as given.

An edit requires a **trusted** file, because locating a passage to replace is a decision and a
decision may be taken only from trusted content. To change a file the agent may not read, the route
is `spawn_processor` plus `write_file`.

### Notebook cells

A `.ipynb` file reads as its cells (see `read_file`), and `cell` changes one by its number instead of
by a passage. `cell` is refused beside `old_text`, `edits` or `replace_all`, on a file that is not a
notebook, and for a number the notebook has no cell for. The notebook is written back in the layout
Jupyter writes, with every other cell, its metadata and the format version kept, and you approve the
diff. Replacing a code cell drops that cell's saved outputs and execution count, since they describe
source that is gone, and the diff shows them going.

## `apply_checkout`

Brings the files a delegate wrote in a kept [checkout](../customize/agents.md#a-checkout-of-its-own)
back into your working directory. **You are asked about every file, and shown the difference from
your own file as it is now,** even where the file's path is one you trust. The question also says so
when this session has written that path in your working directory since the checkout was made, whether
the planner or another delegate did it. A write by a program other than through a redirection, or
through a reference, is not seen, so the absence of that note says nothing about the file.

| Parameter | |
|---|---|
| `checkout` | the number the delegate's report gave the checkout, such as `c1` |
| `paths` | optional: only these files, each one the report named. Without it, every file named |

Only a file the driver recorded a write to, by the name the delegate typed, can come back. A file
written through a reference or by a program other than through a redirection is not found, a link in
a file's place is not followed, and a file over 16 MiB or that is not text is left. The file keeps
the trust label it had in the checkout, so a file written from content nobody vouched for is still
untrusted in your tree. A delegate is never offered this tool.

## `run`

Runs a command line. **You approve the compiled plan before anything runs.** The full reference is
[the run tool](run-tool.md).

| Parameter | |
|---|---|
| `command` | one command line; a newline is refused, since this is a line and not a script |
| `directory` | where to run, inside the workspace or a directory you added ([details](run-tool.md#the-directory-carries-over-and-nothing-else-does)) |
| `deadline_seconds` | how long to wait, defaulting to 300 seconds ([details](run-tool.md#a-line-has-a-deadline)) |
| `background` | start the line and hand back a job name instead of waiting ([details](run-tool.md#leaving-a-pipeline-running)) |
| `stdin_ref` | a reference whose contents are fed to the first program ([details](run-tool.md#filtering-something-the-agent-may-not-read)) |
| `read` | ask for the output in this result; honoured only when [bypassing with no screening](run-tool.md#reading-the-output-in-the-same-result) |

The line is compiled by bravebot's own grammar and never handed to a shell. The prompt shows the
plan: each step, the binary that will run, the directory, and every file the line would create or
replace. A line that writes a file or reads one in through `<` is asked about every time. A line
that only reads what you vouched for runs unasked, and what a line prints stays quarantined unless
you vouched for every step of it (see [the answers](run-tool.md#the-answers-and-how-long-each-one-lasts)
and [the output](run-tool.md#the-output)).

## `read_output`

Asks to be shown what a command printed. You see the output and decide; if you agree, it comes back to
the planner as text.

| Parameter | |
|---|---|
| `ref` | the reference a `run` handed back |
| `offset` | for output the planner may read that was too long for one result, the byte to start from; defaults to 0 |
| `pattern` | for output the planner may read, a regular expression (or a list of them) to search it for instead of paging it |

It works only for output from `run`. A quarantined *file* is not readable this way. This is why
`which`, `find` and `uname` tell the planner nothing until it asks. When you bypass permissions with no
screening, nobody is shown it and it comes back at once, and [`read` on `run`](run-tool.md#reading-the-output-in-the-same-result)
makes the same release without the extra call.

**A confined check reads the output before you are asked**, and the word it gave and the sentence it
wrote are on the screen beside the bytes. The check runs before the question rather than after your
answer, and nothing it wrote goes back to the planner either way. No expectation is sent with it: the
planner asked for the output to be read, not for it to be judged. See [Vetting](../security/vetting.md).

**Output the planner may already read is not asked about.** Where a command's output was one the
planner may read and too long for one result, it saw the beginning and the end, and `read_output`
hands back the rest a page at a time from `offset`. Each page is no longer than
[`run.maxOutput`](../customize/configuration.md#runmaxoutput) and ends with the offset of the next.
Nobody is asked, since there is nothing for you to decide: only the length kept it out. An `offset`
on output the planner may not read is refused before you are asked anything. The refusal says you are
asked only where one answers: otherwise the output comes back as text, or is kept back when output is
vetted automatically. A `pattern` on such output is refused the same way.

**A pattern searches that output rather than paging it.** On output the planner may already read,
`pattern` answers with the lines matching the regular expression, each numbered with its line and
with the byte it starts at, two lines of context either side. The planner passes one of those bytes
as `offset` to read the page from there, so a long log is narrowed without running anything again.
At most 100 matches come back, within the same output cap, and a closing line says how many matched
and where the next undrawn one starts. A `pattern` on output the planner may not read is refused
before you are asked anything, for the same reason an `offset` is: which lines matched and how many
there are would be worked out from bytes nobody vouched for. `pattern` and `offset` together are
refused.

## `vet_content`

Asks to be shown one quarantined slot, after a confined check has read it. This is the one prompt of
its kind the planner asks for by name, rather than one arriving with a read it made for its own reasons.

| Parameter | |
|---|---|
| `ref` | the reference naming the slot |
| `expects` | what the planner expects the slot to hold, in its own words |

`expects` decides nothing. It is sent to the check, so a page can be judged against what it was
supposed to be, and it is drawn on the prompt, so you can see why the planner wants this. It may not be
private.

**Where the content came from is said in bravebot's words, as a path, a URL or a command**, never as a
reference name. A reference means something to the planner and nothing at all to you.

If you agree, the bytes come back as text the planner may read. If you do not, it is told so and told
to work with what it has or to say what it needed, rather than being left to ask again. Nothing the
check wrote goes back either way.

A picture or a PDF is vetted the same way, except that the check is given the file and you are shown a
copy of it to open. If you let it through, it is attached to the planner's next request and is not
returned as text. See [A picture or a PDF](../security/vetting.md#a-picture-or-a-pdf).

Refused for a reference to nothing, for a private `expects`, and for a call from a delegate. A
picture is also refused where the model in use takes no pictures, or where there is nowhere to put a
copy of it for you to open. A reference to a file nothing has read
yet is opened rather than refused, since naming one is the ordinary way to ask about a file the planner
may not read.

See [Vetting](../security/vetting.md) for what the check is, what it may say, and what
[auto-vetting](../security/vetting.md) changes.

## `job_output`

Reports what a [background job](run-tool.md#leaving-a-pipeline-running) has printed since the last look.

| Parameter | |
|---|---|
| `job` | the job name a `run` handed back |
| `kill` | stop the pipeline |
| `wait_seconds` | sit and watch for up to this long ([below](#a-look-may-wait)) |
| `until_exit` | with `wait_seconds`, wait for the job to end and not for its output ([below](#a-look-may-wait)) |

Each look reports what is new, counted in bytes, and whether the job has ended. Asking about a job
that does not exist says so. What is new is counted separately for standard output and standard error,
so a line arriving on one does not hide what arrived on the other, and text from standard error is
marked as such.

A `job` naming a [delegate](#spawn_agent) rather than a job is answered as that: `job_output` neither
reads nor stops a delegate, and its report reaches the planner on its own when it finishes. The refusal
tells the planner to end the round with one line saying what is outstanding, and the turn asks it again
once the report arrives. Only a
name that is neither is reported as no job.

**A finished job reaches the turn without being asked about.** Between rounds the turn checks whether
any of its jobs has ended, and the handle, the exit codes and whatever was printed since anybody last
looked go into the conversation on their own. So a background job that ends quietly is still reported.
The account is given once, by whichever route got there first. Where the output is quarantined it
says how to lift that, as a `run` result does: [`read_output`](#read_output) for this output, and
vouching for every stage of the exact command for the next.

### A look may wait

`wait_seconds` makes one look sit and watch instead of taking a snapshot. It comes back at the first of
four things: output arriving that the planner has not been handed, the job ending, the wait running
out, or the turn being cancelled.

A job that prints more often than a round takes ends that wait at its next line, so waiting for a build
that prints a line per crate costs a look per line. `until_exit: true`, given with `wait_seconds`, waits
for the job to end instead. Output arriving does not end it, including output already waiting when the
call is made. It comes back when the job ends, when `wait_seconds` runs out, when the turn is
cancelled, or when you stop a job of the turn, with what the job printed since the last look in one
answer: where the output is quarantined, that is one reference to all of it. `until_exit` with no
`wait_seconds`, with `kill`, or with anything but `true` or `false`, is refused.

Between 1 second and 10 minutes, and **a value outside that is refused rather than quietly shortened**.
A wait that was silently cut short would come back with silence, and silence carries no length: a
caller that asked for ten minutes and got one would report the same nothing as ten minutes of nothing.
[A deadline](run-tool.md#a-line-has-a-deadline) is clamped instead, because a run that ends says how long it took.

The answer says how many seconds were spent watching and that nothing is watching now, so a wait that
ended early because output arrived is not read as a standing account of the job. A job still running is
reported as running rather than as stopped, and a job that has ended is reported by the codes its steps
exited with rather than as having succeeded.

**A wait cannot outlive the turn.** It is a way to spend part of one turn watching, not a way to be told
about something later. Watching that has to survive a turn is a [`/loop`](commands.md#loop-interval-prompt).

**The output keeps the label its plan was given** when the job started, rather than one worked out
again at the moment it is read. What you have vouched for can change while a job runs, and a pipeline
started before that must not have its output relabelled because of it. So a job's output is
quarantined or not [on the same terms a foreground run's is](./run-tool.md#the-output).

## `fetch_url`

Fetches an `http` or `https` URL. **You approve every fetch, unless a rule names the host.**

| Parameter | |
|---|---|
| `url` | the one URL to fetch |

**Not the tool for a GitHub pull request or issue where `gh` is installed.** The description says so
and the system prompt names the commands that are, which is
[The GitHub CLI](../customize/instructions.md#the-github-cli).

**What comes back is quarantined however you answer.** The body is a reference: the planner may hand
it to [`spawn_processor`](#spawn_processor) or write it to a file with
[`write_file`](#write_file), and it cannot read it or be told what it says. A page saying "ignore
your previous instructions" says it to a processor with no tools.

It is untrusted but **public**, unlike a file of your own: a fetched page is not your data, so writing
it raises no question about confidentiality and there is nothing of yours in it to release.

An `allow` rule matching the host answers the prompt. A `deny` rule **refuses without asking**, so a
host you ruled out is never put to you as a question.

There is no answer at this prompt that trusts a body, because approving a fetch is consent to talk to
a host and says nothing about what that host returns. `a` at a run prompt can trust output: you read
one command and answered for both its effect and what it printed. A host answers every later request
however it likes.

The prompt draws the URL, and on a line of its own the host it will reach. The host is read out of the
URL by the parser rather than taken from what the string looks like, so
`https://example.com@evil.test/` cannot name one site to somebody skimming it and reach another.

**Your answer is bound to that one URL and nothing is remembered.** The next fetch asks again, even
on the same host. Standing permission is a
[`WebFetch(domain:…)` rule](../customize/configuration.md#permissions) you wrote down in advance,
which is naming a host rather than answering a question about one page. See
[A fetch is approved one URL at a time](../security/permissions.md#a-fetch-is-approved-one-url-at-a-time).

**A redirect may not leave the approved host.** Every hop goes through the same gate, and a hop
elsewhere is refused unless a rule allows that host too, since the end of a redirect chain is
somewhere nobody was shown. Before it is reported as refused, you are asked about the host the
redirect named, drawn as a redirect from the host you approved. A yes sends that one fetch again
with that host allowed for it alone and remembers nothing, and a no gives the refusal. Bypass mode
does not answer it, and a `deny` rule refuses without asking. That check applies only while a fetch is in flight: reaching the model
endpoint is this program operating rather than something a turn asked for, so a rule about a website
cannot stop bravebot talking to its own backend.

A hop that keeps the approved host but drops TLS is a separate question, decided for every request that
leaves the process: an `https` chain is not followed into cleartext. See
[Security](../security/security.md).

**A name that resolves to a non-public address is not fetched.** Approving `docs.example.com` approves
a name, and a name resolves to whatever its server says. Each hop's host is looked up once, and if any
address in the answer is loopback, private, link-local (the cloud metadata address among them),
unique-local, carrier-grade NAT or otherwise reserved, the fetch fails before anything is sent and the
connection is made only to the addresses that were checked. A URL whose host is written as an address,
such as `http://127.0.0.1:8080/`, is what you approved and is fetched; `localhost` is a name. Behind a
proxy the proxy resolves the target, so the check does not apply there. A model running on loopback is
unaffected, since this applies to `fetch_url` and nothing else.

**A result names the URL you asked for, never where a redirect went.** A fetch that succeeds names it
as the origin of what came back; one that fails or is refused names it as the request that did not work,
with the kind of failure. Past the first hop the address a request is on is a string a server wrote into
a header, and a result naming that would be handing the planner a server's words with bravebot's
attribution on them. A refusal for leaving the host names the host that was approved rather than the
one the server chose.

A body that is not valid UTF-8 is carried anyway rather than reported as an error, since nothing here
reads it. Bodies are size-capped, and a truncated one says so.

**Every fetch asks for Markdown first.** The request carries one fixed header,
`Accept: text/markdown, */*;q=0.9`, on the first request and on every hop of a redirect. A site that
can serve a page as Markdown serves far fewer tokens than its HTML, and a processor reading the page
pays for every one of them. Nothing in the planner's call or in a server's reply changes the header,
and asking is all bravebot does: converting HTML to Markdown would mean reading the page, which only
a processor does.

Markdown is the only type ranked. A site holding no Markdown answers with whatever it would have
answered without the header, so a page that is only ever HTML arrives as it did before. Ranking HTML
above the wildcard would cost more than sending no header at all, since an endpoint that serves both
a compact form and a full page reads that as a request for the page.

## `download_url`

Saves what an `http` or `https` URL serves to a file in the workspace, byte for byte.
[`fetch_url`](#fetch_url) cannot do this: it decodes a body that is not text lossily and stops at
2 MiB, so a picture, an archive, a PDF or a font comes out corrupted.

| Parameter | |
|---|---|
| `url` | the one URL to download |
| `path` | the workspace file to save it to |

**You are asked about the host, and then about the destination.** The host question and the rules
that answer it are [`fetch_url`](#fetch_url)'s: an `allow` rule matching the host answers it, a
`deny` rule refuses without asking, a redirect may not leave the approved host, and a name that
resolves to a non-public address is not fetched. The destination is put to you every time, whatever
the trust map says about the path, as `url -> path`, and an existing file is shown as an overwrite.
Nothing is sent before you answer. A path outside the working directory and the directories you
opened is refused before either question.

**The bytes go from the network to the file.** They are written as they arrive and are never held by
the agent or shown to the model. The file is recorded as not vouched for, so reading it later is
quarantined as a fetched page is. The model is told the path, the number of bytes and the HTTP
status. The content type the server named is shown to you and not to the model.

**A failed download changes nothing.** The file is staged beside its destination and replaces it
only once the whole body has arrived, so a dropped connection, a stop or a body over the size cap
leaves what was there and no partial file.

The cap is 100 MiB per call and is the [`download.maxBytes`](../customize/configuration.md#downloadmaxbytes)
setting. A delegate is never offered the tool, and plan mode and the
[sandbox mode](../security/security.md#confinement) `read-only` refuse it, since it writes a file.

## `spawn_processor`

Transforms quarantined content the planner was not shown. It spawns an isolated model with no tools,
no memory and nothing to read but the references named. Its output is quarantined as a new reference,
which the planner does not see either.

| Parameter | |
|---|---|
| `reads` | the references to give it, e.g. `["ref:0", "ref:1"]`; at least one |
| `about` | which of those references this call is about; required when `reads` names more than one |
| `instruction` | what to do with them and what to produce |

An answer is for **one** document and may be written **only** to the file the call was about. Where the
planner said nothing and there was more than one input, the answer belongs nowhere and may be written
nowhere.

**Refused once the model has been shown private content.** After [`read_output`](#read_output) or
[`vet_content`](#vet_content) has shown it something private, or a delegate has reported something
private, what it writes is private too, and a processor takes no private instruction, so every
`spawn_processor` call is refused for the rest of the conversation. `run`, `read_output` and the
file tools keep working. Quarantined results made from then on do not suggest a
processor, and the refusal names what still works: a filter such as `grep` or `tail` run with
`stdin_ref` set to the reference, then `read_output` on what it printed, or `vet_content` for a
reference that holds no text.

Everything before the document marker in a processor's reply is a remark for you: it reaches your
screen and stops there, is part of no file, and cannot be another processor's input. An answer with no
marker names no document and can be written nowhere.

See [How Brave Bot works](../how-it-works.md#processors).

## `spawn_agent`

Hands a sub-task to a [delegate](../how-it-works.md#delegates), a second planner with a narrower set
of capabilities, and gets back one report.

| Parameter | |
|---|---|
| `kind` | `reader`, `checker` or `worker`, or the name of a [definition](../customize/agents.md) |
| `task` | the whole of what the delegate is told |
| `each` | optional; starts one delegate per entry, each told `task` followed by its own entry |
| `mcp_servers` | optional; the [MCP servers](../customize/mcp-servers.md) a `worker` keeps, out of the ones its parent may call. `[]` keeps none |
| `budget` | optional; the most tokens each delegate may spend, as `500000`, `500k` or `2m`. It can only lower the budget a [definition](../customize/agents.md) sets |
| `isolation` | optional; `checkout` gives each delegate [a checkout of its own](../customize/agents.md#a-checkout-of-its-own) to work in, and `checkout_with_changes` also copies into it the changes your working tree has now |

| Kind | Holds | For |
|---|---|---|
| `reader` | reading | finding something out |
| `checker` | reading, running programs, and asking a language server | finding out whether something works |
| `worker` | reading, running programs, asking a language server, writing files, and calling the [MCP servers](../customize/mcp-servers.md) its parent may | finishing a sub-task |

The call answers as soon as the delegate has been approved, so the planner has its round back while
the work goes on behind it, and what the delegate says arrives on its own later. Several delegates
can be going at once, each numbered in the order the turn started them, and every report says whose
work it describes.

A name that is neither a kind nor a definition is refused, and the refusal lists the names that would
have worked.

A delegate holds its kind's capabilities **narrowed by its parent's**, so delegation redistributes
authority and never creates it, and a kind asking for more gets a delegate without it. The planner
supplies the task and nothing else. What a delegate is told about itself is a constant its kind
chose, so there is no sentence the planner can write that changes what a delegate *is* rather than
what it is doing.

**`each` fans one task out**, so the shared half is written once and only the differing part (one
path per entry, say) is repeated. Every delegate it starts is one like any other: it is approved on
its own, takes its own number, and holds its own copy of what you vouched for, so a fan-out is
several runs rather than one run several times. A call naming more than eight, or naming none, is
refused and starts nothing. A call that reaches the turn's ceiling on delegates running at once (64
unless [`maxRunningDelegates`](../customize/configuration.md#maxrunningdelegates) says otherwise)
part-way starts the ones that fit and says how many did not start, and why.

**`mcp_servers` takes servers away from a worker.** Without it a worker holds every server its
parent may call, less any its [definition](../customize/agents.md) leaves off. With it the worker
holds only those of them the list names, so a worker sent to fix a build can be started holding no
mail server.
A name is the server's part of its tools' names, as in `mcp__gmail__search`. A name the turn holds
no grant for is refused and starts nothing, and the refusal lists the servers it holds. The list
never adds a server.

**`budget` stops a delegate at a spend.** A delegate that has used its budget in tokens loses its
tools for its next request and answers with what it has, and the report arrives with a sentence
saying the budget was spent and the work may not be finished. A call's figure never raises the
budget its definition sets, and in a fan-out it applies to each delegate separately. A delegate that
starts delegates of its own gives them at most what it has left, divided between the ones one call
starts, and what a child spends counts against it. One with nothing left starts none. A request that has started finishes, so a delegate can pass its budget by one request.
The figure is tokens only, never credits.

**`isolation: checkout` keeps delegates out of your working tree.** Each delegate works in a new
checkout of the last commit, so delegates writing at the same time do not edit or build in one tree.
Changes you have not committed are not in it, and what it writes stays there until
[`apply_checkout`](#apply_checkout) brings it back. It is refused for a `reader` and for a delegate
already in a checkout. A delegate in a checkout is not offered [`lsp`](#lsp). Checkouts share
remote-tracking refs and tags with your working directory and with each other, so the planner is
told to run a `git fetch` once rather than have each delegate run one.

**`isolation: checkout_with_changes` gives the delegate the tree you have.** It is a checkout as
above, with the changes in your working tree copied in as they are when the checkout is made: the
files the session wrote, and the paths `git status` lists as changed where the session trusts the
whole tree. A file you deleted is deleted there. A file a deny rule covers, a link, a file over
16 MiB and anything past 1,000 files or 64 MiB is left as the commit has it, and the delegate and the
planner are told which paths were carried, which were not and why. Nothing is written in your working
tree, and you are not asked, since the copy goes into the checkout bravebot made. Use it for a
`checker` that has to build what the planner just wrote.

The delegate cannot see the conversation the task came from, so a task that leaves something out is a
delegate that never learns it. It cannot come back for more, since there is no channel to ask
along. A run whose own context has met something untrusted cannot delegate at all.

**Delegation saves context, never an approval.** Every write and every run a delegate makes reaches you
with its own single-use endorsement, so you see the path and the diff whoever proposed them, and every
call a worker makes to a server's tool is put to you as the turn's own is. What you
vouched for inside one comes back to the session, because that answer was about your machine rather
than about the run that happened to be going.

Nothing but the report crosses back: the exchange, the tool results and the quarantine end with the
delegate, and a reference minted inside one names nothing afterwards. A delegate is offered none of
[`ask_user`](#ask_user), a task list, or [`fetch_url`](#fetch_url), so it puts no question of its own
to you, replaces nothing on your screen, and reaches no host.
Naming one of them anyway is refused rather than run, since a model naming a tool it was never offered
is ordinary. What it could not settle goes in the report, and the planner asks.

A delegate is offered this tool itself, down to three levels below the turn. One at the bottom is
not offered it, and nor is one whose [definition](../customize/agents.md) names its tools and
leaves this one out. A call from either anyway is refused. Every delegate in the tree counts against the
turn's one ceiling, and each is approved and narrowed by the run that started it.

Fetching is out because a delegate reaching a website would stop you to approve a host for a sub-task
you never set up. Every kind does reach the network, since a planner is a model call and that request
is egress like any other, and that is the whole of what the capability buys it.

:::note
The confirmation for a write shows the path and the diff, as it always does, but it does not say that
a delegate rather than the turn is asking. With several running, reading only the prompt means
approving a change whose reason is one of the tasks you did not read.
:::

## `load_skill`

Reads one of the skills the planner was listed, or one `search_skills` found.

| Parameter | |
|---|---|
| `name` | the name exactly as it was listed or as `search_skills` returned it, e.g. `commit-style` |

The name selects from a set fixed before the turn started and **never becomes a path**: a name holding
`../` matches nothing and the call is refused. A name close to a real one is refused rather than
guessed at. A skill whose file says `disable-model-invocation: true` is not in the set, so asking for
it is refused the same way; you load it by naming it in your own line. See
[Skills](../customize/skills.md).

Where the skill keeps other files beside its `SKILL.md`, the result ends with the skill's directory
and their names. Where a project's skill directory is a link to another directory in the project,
the result also names the directory it lands on, which is where `list_files` shows the files. Nothing
outside the project is named. Once a skill of your own is loaded, [`read_file`](#read_file) can read those files
for the rest of the turn; a project's skill is read through the trust map like any other file in the
project.

## `search_skills`

Finds a skill of your own or the project's by a word or phrase. Only a skill built into bravebot
itself is listed for the planner by default; the rest are found this way.

| Parameter | |
|---|---|
| `query` | one or more words; a skill matches only where every word is in its name or its description |

A match's name and description are returned in full, never cut, with the same words
[`load_skill`](#load_skill) then loads the body of. A query that matches more than 25 skills
returns the first 25, in the order they would be listed, and says how many more matched: narrow
the query rather than widening it. See [Skills](../customize/skills.md).

## `ask_user`

Puts up to four questions to you and waits. More than four is refused whole rather than trimmed.

| Parameter | |
|---|---|
| `questions` | at most four, each with a `header`, a `question`, optional `options`, and `multiple` |

Questions are put one at a time. You may choose an option, answer in your own words, or skip. A
skipped question is an answer to work with rather than a reason to ask again. An answer is remembered
for the session, question by question.

In the terminal, a series of more than one ends in a review that lists each question beside your
answer. Nothing has been sent until you press Enter there. Left goes back a question, a number
reopens that question from the review, and either brings back what you had answered. A single
question has no review.

With [`askIdleSeconds`](../customize/configuration.md#askidleseconds) set, the terminal closes a question
that sits that long with no key pressed, and every question behind it. The planner is told that no
answer was given in the time allowed and that you may be away, not that you declined, and the answers
you had already given stay. A question closed this way is not remembered, so the planner can ask it
again.

**Asking stops once the planner's context has met something untrusted**, because at that point the
question itself could have been shaped by content nobody vouched for. A quarantined read does not stop
it asking, since a reference carries no instruction. Where nobody can be asked, as in a one-shot run,
every question is declined rather than answered on your behalf.

This tool is for what the planner cannot find out itself: which of two approaches, whether something
is in scope, which of two plausible files you meant. Never for a fact about the machine.

## `propose_plan`

Offered in plan mode only. Shows you the finished plan and asks how to proceed.

| Parameter | |
|---|---|
| `plan` | the whole plan, in the planner's own words |

You can accept edits, ask before each edit, or keep planning. The first two leave plan mode for that
mode and the planner starts the work in the same turn. Anything else, including a typed reply, keeps
plan mode and tells the planner the plan was not approved. Bypassing is never offered. Where nobody
can be asked, or the planner's context has met something untrusted, the call is refused.

## `request_path`

Asks you to let the programs a `run` starts reach one path they otherwise cannot, for the rest of the
session. A command that fails with `Operation not permitted` on a path outside the directories the
session was opened on is the usual cause.

| Parameter | |
|---|---|
| `path` | one absolute path, or one beginning `~/`; it must exist and may not hold `*` or `?` |
| `write` | `true` to let programs write it as well as read it; defaults to `false` |

You are shown the path, whether it is to be read or written, and the planner's reason, and you
answer yes or no. A call with no reason, or a blank one, is still put to you. A yes lasts for this session, is not saved, and does **not** mark the directory
trusted, as [`/add-dir`](commands.md) does. It does not let `read_file`, `write_file`, `edit_file`,
`list_files` or `search` reach the path either: those are opened with `/add-dir`, and a refusal from one of them for a path outside the workspace names `/add-dir` alone. `/reach paths` lists what you have allowed and
`/reach paths remove <number>` takes one back; `/status` lists them too. With permissions bypassed
the answer is yes without a question, and the directory is also opened for those five tools and
trusted, as `/add-dir` would, and the result tells the planner whether they now reach it: `/add-dir close` and `/clear` close it, and `--resume` carries it. With permissions bypassed the file tools do not need the
request: every tool that names a path (`read_file`, `write_file`, `edit_file`, `list_files`, `search`, `repo_map`,
`read_git`, `download_url`, a `run` directory and the target of a redirection) reaches a path
outside the workspace wherever your account can, under every sandbox mode, with no directory opened or trusted,
and it ends when you leave bypass. The paths this page lists as refused here are refused to them too, so are
your `denyRead` and `denyWrite` paths, `permissions.readsStayInWorkspace`, writes under `read-only` and a workspace you
have not trusted. `/status` lists the directories reached. On macOS and Linux with the sandbox mode `workspace-write`, the programs
every `run` starts, a delegate's included, are also given those paths, minus your home directory as a whole, the credential
locations and your `denyRead` and `denyWrite` paths, so a sibling of the working directory does not
fail with `Operation not permitted`. A
directory inside the working directory, or one that `/add-dir` would otherwise refuse, is not opened,
and the planner is told so. One that holds the working directory ends checkouts while it is open.

The same paths are refused as an `allowWrite` entry would be, whatever you would answer, and you are
not asked: `~`, `/`, a drive root, your home directory and any directory above it, `~/.ssh`,
`~/.bravebot`, the credential locations and any directory that holds one, such as `~/.config`. A request to read `~/.bravebot/sessions` or a path inside it is the one exception, and a request to write there is refused like the rest of `~/.bravebot`. A path your `denyRead` or `denyWrite` entries cover is
refused as well. Nothing is accepted under the sandbox mode `off`, in a workspace you have not
trusted, in a run with nobody to ask, or in the desktop app, which has no question for it yet. No
settings file can allow one in advance. A request to write is also refused, whatever the mode, under
the sandbox mode `read-only`, and for the shell startup files, the user job directories and git's
global configuration that [the security page](../security/security.md#confinement) lists. A refused
request to write says what is true of the path: it holds credentials, a write there could leave code
that runs later, it is too broad, or your deny list covers it. It does not say which setting decided.

## `todo_write`

Records the task list for what the planner is doing.

| Parameter | |
|---|---|
| `todos` | the complete list, each with `content` and `status` |

The whole list every time: it replaces the previous one. There is no routing here, because nothing is
touched. An unrecognised status reads as outstanding work. A task marked `cancelled` is struck
through and counted apart from the finished ones.

A turn whose last list still has a task pending or in progress when the planner answers is not
ended there. The planner is told how many tasks are open, and nothing about them, and is asked to
send the list again with each task marked completed or cancelled. Where work remains it leaves the
tasks open and says in its answer what is left. This happens once for each list: a list the planner
sends in answer to the question is not asked about, in that turn or in a later one. A list the
planner wrote in a session that has read something untrusted is not counted. A reply that stopped at
the output limit ends the turn as it stands.

## `schedule_next`

Says when this turn should be asked again: the pace of the next tick of a self-paced
[`/loop`](commands.md#loop-interval-prompt), or, outside a loop, the first later look at something,
which starts a loop over the line you typed. It is offered only where that later look can actually
happen; a call from any other turn is answered the way any unoffered name is. A turn that runs a prompt
bravebot wrote for a command, as [`/init`](commands.md#init), [`/review`](commands.md#review-target-focus)
and [`/help`](commands.md#help-question) do, is not offered it, so those cannot start a loop that repeats the fixed prompt.

| Parameter | |
|---|---|
| `delay_seconds` | how long to wait; routing, and required unless `stop` is true |
| `noop` | whether this tick found anything; routing, and required |
| `stop` | true to end the self-paced loop this tick belongs to, with no further tick; routing, and offered only to a tick of a self-paced loop |
| `reason` | what the turn is waiting on, in its own words; content |

**There is no argument for what the next run asks.** The prompt is the line you typed, and it is sent
again unchanged.

The wait is held between a minute and an hour **before** it is reported back, so the number the
planner is told is the number it is getting. A call missing the delay (unless it sets `stop`) or the verdict is refused rather
than filled in, since the count of quiet ticks you are shown is built from the verdict. `reason`
reaches your screen and stops there.

## `watch_file`

Arms a standing watch on one file. The result confirms the watch exists.

| Parameter | |
|---|---|
| `path` | the one file to watch |

**The path is the only argument**, and that is deliberate. No interval, no condition, no sentence for
the firing to say, and no second path. Reading the call tells you which file this session may be told
about, and that is the whole of the decision. A field for what a firing should say would let a turn
write its own next prompt, and a field for how often to look would make the latency the planner's to
choose.

**Nothing is asked that a read would not ask.** Inside the working directory a watch is the promotion a
read of the planner's own choice of file already gets; outside it, whatever answer already stands. A path
a read would be refused is a watch that is refused. Asking to be told when a file changes is asking for
less than reading it, so a prompt of its own here would be a second question to somebody who answered
the first.

The path must name a file that **exists now**. A directory is refused, because what changed inside one
is a filename the filesystem produced and a firing may carry nothing off the filesystem. A name with
nothing at it is refused because the first look is taken when the watch is armed, so there would be
nothing to compare against and the first look that found the file would report a change it never
underwent.

A call the session cannot honour is refused with the reason rather than armed silently: a session
already running a [`/loop`](commands.md#loop-interval-prompt) or working towards a goal, and a session
already holding as many watches as it keeps, each say which of the three it is.

See [Watches](../using/watches.md) for what a watch then is, what a firing puts in the conversation, how
long one lives and what ends it.

---

## `advisor`

Puts a question to a second model that is shown the conversation. The result is its reply, as text.

| Parameter | |
|---|---|
| `question` | what the planner wants the advisor's view on |

The advisor is sent the same request the planner was sent on that round, followed by the question, and
is offered no tools. It can only advise. The planner does not choose the model: the session does, with
[`--advisor`](cli.md#--advisor-name), the [`advisorModel`](../customize/configuration.md#advisormodel)
setting or [`/advisor`](commands.md#advisor-model--off).

The reply is labelled from the planner's context, as the planner's own words are. While that context
has met nothing untrusted the planner reads it. Once it has, the planner is given a reference to it
like any other content nobody vouched for.

A turn may ask at most three times, and a fourth call is refused without a request. Each call is
counted in the turn's tokens and recorded in the trail with its model and cost. A call that fails is
reported to the planner as a category of failure, without anything the service said.

---

## `remember`

Keeps one fact in your [memory](../customize/memory.md) on the planner's own judgement.

| Parameter | |
|---|---|
| `text` | the fact, in the planner's own words |
| `key` | a slug naming the fact, such as `release/forge-poll` |
| `scope` | this project, or every project |
| `supersedes` | the key of a fact this one replaces |
| `new` | keep this as a new fact even though a close one exists |

The text is released into a fact only while the planner's context has met nothing untrusted, and the
tool offers no reference field, so a quarantined read cannot be kept. A fact saved from a private
context is kept private. A key that is not a slug is refused before anything is written. Saving over
a key keeps the fact's creation date and tags; a fact you asked to be remembered is never overwritten,
and a fact another one replaced is not saved over.

A save that is close to a kept fact is refused, naming that fact's key and how much of the new text it
holds, and the answer to every save lists the facts above that share. No fact's text is quoted, but
the answer still takes the confidentiality of every fact it names, so a private fact cannot be probed
by guessing and the result sent to a public surface.

It works in every permission mode, plan mode included, because it writes nothing in the workspace.
Each write is backed up first, so [`/undo`](commands.md) takes it back.

## `recall`

Reads your kept facts. It takes `query` or `key`, never both and never neither.

| Parameter | |
|---|---|
| `query` | words to look for, with a quoted phrase matched as written |
| `key` | the key of one fact, to read it whether or not another replaced it |

A query ranks the live facts of the project and of every project together and answers with at most
eight, each with its key, a strength, the date it was last verified and its text. When the best
strength is under 35 the answer begins by saying probably nothing is stored, and the planner is told
to say so rather than assume. For each of the top three results, the facts its text links to with
`[[key]]` follow, one hop and at most five, each marked with the fact it came from. A key that is
superseded says by which key.

Every fact an answer names gives the planner's context that fact's confidentiality from then on, so
reading a private fact cannot be followed by sending it somewhere public. That holds for a query over
every fact it ranked, shown or not.

## `propose_rule`

Puts one standing rule to you, and writes it only if you say yes.

| Parameter | |
|---|---|
| `text` | the rule, in the planner's own words |
| `scope` | `home` for every project, or `project` for the one you are in |
| `reason` | one line saying why |

You are shown the text, the file the scope names and the reason, and you can keep the rule, refuse it,
or type your own wording, which replaces the text. The choices are written by bravebot, never by the
planner, and a skipped question writes nothing. Each question is asked afresh, so an earlier yes is
never reused, and no permission mode answers it for you. Where nobody can be asked, as in a one-shot
run, the proposal is refused.

The rule is appended as one line, `- <text> (<date>) why: <reason>`, to `~/.bravebot/AGENTS.md` for
`home` or to the project's `rules.md` for `project`, after the file is backed up, so `/undo` takes it
back. A call from a context that has met something untrusted is refused before you are asked. Where
the text or the reason came from private material, you are offered only refusing or typing the rule
yourself, and what is written is your wording with `why: typed by the person`.

---

## `message_session`

Sends a message to another background session on this machine. The result says the message is queued in
that session's process.

| Parameter | |
|---|---|
| `target` | the name of the session, as you gave it to the planner |
| `text` | what the planner wants that session's planner to be told, at most 4,000 characters |

You are asked before the first message to a session in a turn. The question shows the session's name,
its state and its working directory as the list of background sessions holds them, the whole text, and
the label it is sent under. A background session holds the question until you attach. You can answer
`y` to send that one, or `t` to send that one and the later ones to the same session until the turn
ends. Nothing else sends, and no permission mode answers the question for you.

The other session's planner is told the text is advice from another session, not your words. The text
grants it no permission and answers nothing it is waiting on. A message starts no turn. It is read
at the start of that planner's next request, so a session that is idle or waiting for you reads it later.
The planner is not told whether it was read. A session holds eight waiting messages at most and
refuses a ninth as full. A message sent from a private context makes the receiving session's context
private. A message that is not labelled trusted reaches the receiving planner as a reference.

The target is a name, so a name that no session has, that two sessions have, that is this session's own,
or that belongs to a session no process is running is refused, and so is a text over the limit. A
refusal does not ask you. A turn may put at most ten messages to one session, counting the ones you
declined. A session that has met content nobody vouched for sends nothing at all.

---

## Before adding a tool

Every new tool must have a routing field. A tool whose destination cannot be separated from its
payload does not belong on this surface. That is also why the built-in tools stay native rather than
arriving over MCP: an opaque call erases the split between the part that decides where a call lands
and the part that is merely carried, and these tools depend on it.
