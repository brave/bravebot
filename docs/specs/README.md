# Specs

Each file in this directory is a mini-spec for one topic within the bravebot system.
Spec changes, additions, and removals are closely reviewed by humans.

If a mini-spec disagrees, that is a bug in the spec that should be fixed.

Specs have automation attached which verifies that there is coverage of functionality and also
that functionality matches specs. It drafts a bug for every problem it finds, showing what went
wrong rather than describing it, and a person reads the drafts and says which get posted.

## The specs

| Spec | Id | Clauses | Topic |
|---|---|---|---|
| [labels.md](labels.md) | `LABEL` | 10 | the lattice, taint, who may read what, and how a first label is assigned |
| [routing.md](routing.md) | `ROUTE` | 8 | where an effect may land and what may decide it |
| [trust-map.md](trust-map.md) | `TRUST` | 21 | which paths the user vouched for, what a write does to that record, and how long an answer lasts |
| [permissions.md](permissions.md) | `PERM` | 15 | rules written in advance about what to ask about and what to refuse |
| [processors.md](processors.md) | `PROC` | 12 | the one component that reads untrusted content, and what it may do with it |
| [vetting.md](vetting.md) | `CHECK` | 14 | checking quarantined content for an injection attempt before every prompt that would promote it, so a person deciding has a second opinion |
| [delegation.md](delegation.md) | `DELEGATE` | 21 | a second planner, narrower than the first, and what crosses back from one |
| [turns.md](turns.md) | `TURN` | 5 | how long a turn may go on, what happens when it does not stop, and what is said when it produces nothing or checks nothing |
| [prompting.md](prompting.md) | `PROMPT` | 10 | every moment the system stops and puts something to a human, and what an answer grants |
| [permission-modes.md](permission-modes.md) | `MODE` | 10 | a standing answer to those prompts: accepting edits, planning, or asking about nothing at all |
| [naming-files.md](naming-files.md) | `NAME` | 7 | writing `@path` in a prompt: what it puts into the turn and what it vouches for |
| [pasting.md](pasting.md) | `PASTE` | 9 | what Ctrl-V puts into a turn, text or picture, and on what footing |
| [dropping.md](dropping.md) | `DROP` | 10 | what dragging a file onto a window puts into a turn, and on what footing |
| [shell-mode.md](shell-mode.md) | `SHELL` | 5 | the `!` prompt: a line the user typed, and why the planner can never reach it |
| [skills.md](skills.md) | `SKILL` | 11 | `AGENTS.md` and skills: what a skill file is and what each source is trusted for |
| [instructions.md](instructions.md) | `INSTR` | 9 | which instruction files are looked for, where, in what order, and where what they say ends up |
| [cli.md](cli.md) | `CLI` | 15 | running without the interactive interface: one-shot tasks, piped input, and `doctor` |
| [manifest.md](manifest.md) | `MANIFEST` | 11 | plan the whole run first, then execute it with no model in the control path |
| [terminal-input.md](terminal-input.md) | `INPUT` | 34 | what the user types into: the box, the keys, and where a terminal's own limits show through |
| [commands.md](commands.md) | `CMD` | 8 | a line beginning with `/`: where one may come from, when a line is one, and what it does to the line |
| [terminal-transcript.md](terminal-transcript.md) | `VIEW` | 23 | what is drawn back: the transcript, a resumed session, and how content reaches the screen |
| [watching.md](watching.md) | `WATCH` | 21 | work done outside the transcript: a delegate's own lines, what a command printed, a question asked beside it, and the mode Ctrl-L opens over them |
| [scroller.md](scroller.md) | `SCROLL` | 9 | reading back through what happened: the mode Ctrl-O opens over the transcript, and the keys inside it |
| [credential-protection.md](credential-protection.md) | `CRED` | 25 | where credentials come from, which of them may be held at all, and what each tier owes |
| [premium-credentials.md](premium-credentials.md) | `PREM` | 9 | importing a Leo Premium subscription and spending its credentials |
| [sandboxing.md](sandboxing.md) | `SANDBOX` | 11 | operating-system confinement for processes running code we did not write |
| [mcp.md](mcp.md) | `MCP` | 9 | tools that come from outside this repository, and what they are allowed to do |
| [mcp-servers.md](mcp-servers.md) | `SERVERS` | 14 | how a person declares one of those servers, what that declaration is trusted for, and what is asked before a tool from one runs |
| [hooks.md](hooks.md) | `HOOK` | 8 | a command a person asked to have run when something happens |
| [network-egress.md](network-egress.md) | `NET` | 9 | every request that leaves this process, and what comes back |
| [backends.md](backends.md) | `BACKEND` | 42 | which service answers a request, and what a person may choose between |
| [compaction.md](compaction.md) | `COMPACT` | 12 | shortening a long conversation into a summary of itself, in the request only |
| [loop.md](loop.md) | `LOOP` | 15 | sending one prompt again and again until somebody stops it |
| [goal.md](goal.md) | `GOAL` | 17 | one condition a person set, judged after every turn, until it holds |
| [file-watches.md](file-watches.md) | `FSWATCH` | 12 | a standing watch on one path, firing with no turn running to notice it |
| [sessions.md](sessions.md) | `SESSION` | 29 | what is kept between runs: the record of a session, the questions asked beside it, and the prompts a person typed |
| [state-directory.md](state-directory.md) | `STATE` | 3 | `~/.bravebot`, and who on the machine may read what is written into it |
| [incognito.md](incognito.md) | `INCOG` | 8 | a session that runs normally and adds nothing to `~/.bravebot` |
| [trace.md](trace.md) | `TRACE` | 6 | what is recorded about every decision the system makes, and what that record may contain |
| [localization.md](localization.md) | `LOCALE` | 7 | every word said to a person, and which of them change with the reader's language |
| [layering.md](layering.md) | `LAYER` | 6 | which crate is allowed to do what |
| [releases.md](releases.md) | `RELEASE` | 12 | what names a version, what starts a release, and what an installer trusts about what it fetched |
| [updates.md](updates.md) | `UPDATE` | 10 | learning that a newer version is out, and the line that installs it |

## The tools

One spec per tool. [tools/tool-surface.md](tools/tool-surface.md) is the table of all of them and
the routing-versus-content split they share.

| Spec | Id | Clauses | Tool |
|---|---|---|---|
| [tools/tool-surface.md](tools/tool-surface.md) | `TOOL` | 3 | the surface every tool shares |
| [tools/read-file.md](tools/read-file.md) | `READ` | 7 | `read_file` |
| [tools/list-files.md](tools/list-files.md) | `LIST` | 5 | `list_files` |
| [tools/search.md](tools/search.md) | `SEARCH` | 9 | `search` |
| [tools/lsp.md](tools/lsp.md) | `LSP` | 10 | `lsp` |
| [tools/write-file.md](tools/write-file.md) | `WRITE` | 4 | `write_file` |
| [tools/edit-file.md](tools/edit-file.md) | `EDIT` | 4 | `edit_file` |
| [tools/spawn-processor.md](tools/spawn-processor.md) | `SPAWN` | 4 | `spawn_processor` |
| [tools/spawn-agent.md](tools/spawn-agent.md) | `AGENT` | 5 | `spawn_agent` |
| [tools/run.md](tools/run.md) | `RUN` | 20 | `run` |
| [tools/command-line.md](tools/command-line.md) | `CMDLINE` | 16 | `run`'s command line, compiled rather than interpreted |
| [tools/read-output.md](tools/read-output.md) | `OUTPUT` | 3 | `read_output` |
| [tools/vet-content.md](tools/vet-content.md) | `VET` | 3 | `vet_content` |
| [tools/fetch-url.md](tools/fetch-url.md) | `FETCH` | 6 | `fetch_url` |
| [tools/load-skill.md](tools/load-skill.md) | `LOAD` | 3 | `load_skill` |
| [tools/todo-write.md](tools/todo-write.md) | `TODO` | 2 | `todo_write` |
| [tools/schedule-next.md](tools/schedule-next.md) | `SCHED` | 5 | `schedule_next` |
| [tools/watch-file.md](tools/watch-file.md) | `ARM` | 6 | `watch_file` |
| [tools/ask-user.md](tools/ask-user.md) | `ASK` | 8 | `ask_user` |

Topics with no spec yet are ordinary code. Adding one is how a topic becomes review-required.

## Format

Front matter, then numbered clauses. Everything outside a clause is commentary and binds nobody.

- **`id`** is a short prefix. Clause ids are `PREFIX-N`, allocated in order and never reused and
  never renumbered, because a commit message, an issue, and a test name all point at one. A
  withdrawn clause stays, marked withdrawn, and says what replaced it.
- **Every clause carries an anchor**, `<a id="PREFIX-N"></a>` on the line directly above its
  heading, so `labels.md#LABEL-3` is a link that keeps working. The anchor GitHub generates from
  a heading contains the title, so it breaks the moment somebody improves the wording, which is
  the moment an issue or a commit pointing at that clause most needs the link to survive.
- **`governs`** lists the paths this spec decides. A diff touching one of them is reviewed against
  this file. Anything under no spec's `governs` is ordinary code and reviewed as such.
  **Name the files, not the crate.** A pattern matching a directory wholesale puts a spec's name on
  every file that lands in it afterwards, which reads as review-required and is not: whoever added
  the file chose nothing, and nobody decided the spec reaches it. Enumerating costs a line when a
  module is added that the spec does decide, and that line is the decision being made where a
  reviewer sees it. The cost is the other way: a new module is under no spec until somebody adds it,
  so a file can sit ungoverned without anything saying so. That is the direction to fail in, because
  a spec claiming reach it has not been read against is worse than one that plainly has a gap.
  Where a pattern is already a glob, as `crates/*/Cargo.toml` is, what it matches is one file per
  crate that cannot be added without a crate being added.
- **`guards`** lists symbols whose every use is review-required. An entry may also pin where it is
  used, as a `sites:` list of `path: count` items, and the check fails when the tree and the list
  disagree in either direction. The count is how many times the symbol occurs in that file's code:
  two uses on one line are two, one call rustfmt wrapped across three lines is one, and a comment
  naming the symbol is not a use of it at all. The line defining the symbol counts as one of those
  occurrences. A method is counted where it is called on a receiver too, since `gate.open()` is how
  most call sites read; an associated function has no receiver, so `Labelled::new` counts its
  definition and every place the qualified name is written, and nothing besides. A qualified
  symbol has to be defined where its qualifier owns it, in an `impl` naming the type or, for
  `home::write_file`, at the margin of the module of that name: a method another type happens to
  spell the same way is not this one, and a guard whose qualifier defines nothing counts no uses
  at all rather than that type's. A count rather than a line number, so moving a call inside a
  file changes nothing, while adding or removing one is an edit to this spec that a reviewer
  sees. `make check-spec` prints the number it found, which is the number to record.
  Within one spec, either every entry pins its sites or none does: an unpinned entry beside pinned
  ones reads as though it were checked too.
- **`documented-by`** names the pages under `docs/website/docs/` that describe this spec to somebody
  using Brave Bot, written from the repository root the same way `governs` is, so one front matter
  never means two different things by a path. A decision per spec, not per clause: a page is written
  for a reader with a task, so it covers a topic and rarely lines up with a single clause.
  `none` is a valid answer and says why in brackets, in one of two forms the check leaves alone but a
  reader can grep for: `none (internal: ...)` for behaviour nobody using Brave Bot acts on, and
  `none (gap: ...)` for a page still owed, which is the site's backlog.
- **`verified-by:`** lines name the tests that pin a clause, as `crate::module::test_name`. The
  coverage check reads them, fails when a name does not resolve to a test that exists, and drafts a
  bug for any clause whose value is `none`. `by-construction` is for a clause nothing can execute,
  such as a crate having no dependencies, and says in brackets what makes it hold.
  **A named test is a Rust test.** The check resolves a name against the `#[test]` functions in
  `crates/`, so a test written in anything else cannot be cited however it is spelled. A clause about
  behaviour only another language implements therefore has two honest forms and no third: a Rust test
  that observes the behaviour from outside, which is what a clause about a separate process gets, or
  `verified-by: none`, which lands in [../../unverified-clauses.txt](../../unverified-clauses.txt)
  where the gap is a line in a diff. Citing a test the check cannot resolve is neither.
  A clause is still allowed to be about such behaviour: what the check decides is how a clause is
  pinned, not what one may say. What it means in practice is that the clause worth writing is the one
  stating the property this side can observe, since that is the one a test can hold to.

A clause is one rule, stated so that a reader can tell whether a given diff obeys it. If a clause
needs an "and" it is usually two clauses. A set of rules that reads best as a table is one clause with a
table in it, never a clause id per row: ids must be greppable one to a heading.

**Each spec stands alone.** A clause id is only ever cited inside its own file, where a reader can
scan for it. Never cite another spec's clause: somebody landing here first has no idea what
`TRUST-5` is, and chasing an id across files to understand one rule is how a set of small documents
becomes worse than one big one. Where a clause leans on something another spec owns, state the fact
plainly in a few words. "The same 'most specific wins' rule the trust map uses" costs a line and
needs no second tab.

**Point at documents, not at ids.** When a whole topic lives elsewhere, link the file by name:
`[trust-map.md](trust-map.md)`, not `` `TRUST` ``. A bare prefix means nothing to a first-time
reader, and it cannot be clicked.

**Clauses describe behaviour, not implementation.** Say what happens and why, never which function
or type does it. A clause naming a symbol becomes wrong the next time somebody renames one, and it
pins the code that exists rather than the behaviour the code owes. Identifiers belong in `governs`,
`guards` and `verified-by`, where tooling reads them and a rename is caught by the check rather
than by a reader noticing.
