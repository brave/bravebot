---
sidebar_position: 2.2
title: Permission rules
description: The deny, ask and allow lists that settle in advance whether you are asked or an action happens at all, and what a rule covers and does not.
---

# Permission rules

The `permissions` block of `~/.bravebot/settings.json` holds three lists (`deny`, `ask` and `allow`)
saying which actions to refuse outright and which to stop and ask you about. They are the same three
lists Claude Code keeps, with the same spellings, so a block copied out of `~/.claude/settings.json`
governs bravebot unedited. See
[Configuration](../customize/configuration.md#permissions) for how a rule is written.

**A rule decides whether you are asked, and whether an action happens at all. Nothing else.**

**A deny rule refuses before the file is opened or the program is looked for.** A denied file is not
read, not enumerated, not searched and not written, and a `Read` deny rule also stops a write to the
path it covers. A file whose contents are off limits is not protected if it can be overwritten.
Naming the path through a reference reaches the same refusal, including on the one route allowed to
read what nobody vouched for: a processor is handed no denied file either. The planner is told the
rule refused and that retrying is not the answer.

**A rule covers the file, not only the spelling used to ask for it.** A call is checked under the name
it gave and again under the name it reaches once every link on the way is followed, so a link to a
denied file is refused, and an `ask` rule still asks for a write that comes through one. On Linux and macOS a
file with more than one hard link is also checked under each other name the open directories hold for
it, so a hard link to a denied file is refused in a read and in a write. If that search hits its cap or
cannot list a directory while a `Read` or `Edit` deny rule is in force, the call is refused. The second
name can add a refusal or a question and never removes one. A `deny` or `ask` rule written through a
link covers the place the link reaches: with `linked` a link to `real`, `Read(linked/**)` refuses
`real/secret.txt`. A `deny` or `ask` rule written as a full path, or from `~/`, that lies inside the
project also covers the file under the relative name the file tools ask by, so in a session opened in
your home directory `Read(~/.ssh/**)` refuses `.ssh/id_ed25519`. On Linux and macOS a `deny` or `ask`
rule also covers the name a file has under a second path to an open directory, such as a macOS
firmlink or a Linux bind mount once you have opened it with `/add-dir`; the file is matched by its
device and inode. An `allow` rule is matched as written, so a grant reaches no further than the
spelling you approved.

- **Unicode spellings.** A `deny` or `ask` path rule covers every spelling Unicode treats as the
  same name, so a rule written with a precomposed `é` refuses the file named with a letter and a
  combining accent, and the reverse. A compatibility form is not the same name: a full-width letter
  is not the plain letter. An `allow` rule is matched as written.
- **Case.** On a volume that treats `.env` and `.ENV` as one file, a rule written one way covers the
  other, in every list. On a volume that keeps them apart, or where it cannot be told, a rule about
  `Docs` says nothing about `docs`.
- **Windows names.** A name that reads as something other than the file it spells, such as a stream, a
  device, a trailing dot or space, or an 8.3 short name, is refused outright, in a call's path or a
  redirection's target, since no rule can be matched against it. A path on a drive is a full path:
  `Read(//D:/added/**)` covers `d:\added\secret`.
- **Hosts.** A host written with a trailing dot, `https://evil.example./`, is the host without it, so
  a `WebFetch(domain:evil.example)` rule covers both.
- **Commands.** A command rule is matched against the program and each argument as the line was split,
  not against the line run together. A space in an `allow` rule covers only the gap between two words,
  so `Bash(ls *)` does not cover `"ls /x"`, a script named `ls /x`. In `deny` and `ask` a space also
  covers a space inside a word, and a `*` covers any text in every list.
  A `deny` or `ask` rule also covers the program under a leading `timeout`, `time`, `nice`, `nohup`,
  `stdbuf`, `command`, `builtin`, `noglob` or `env`, so `Bash(git push *)` refuses `timeout 30 git push`.
  An `allow` rule does not look through them, and `npx`, `devbox run` and `docker exec` are not looked
  through by any list.
- **Redirections.** A `deny` rule on a file refuses a redirection to it or from it, as it refuses a
  write. It holds when the redirection reaches the file through `..`, as `config/sub/../app.env`
  does, whether or not a link is on the path.
- **Files read on your behalf.** A denied `AGENTS.md`, a file it points at, a skill or an agent
  definition is left out of the turn, and you are told. A checkout cannot put a denied file into the
  system prompt by committing a link to it.
- **Files you name.** A file named with `@` or `--file`, or dropped, is checked against the rules
  before it is vouched for or opened. A covered one is left out of the turn and you are told.
- **`lsp` answers.** A location in a covered file is left out of the answer, without a count of how
  many were left out. Hover text names no file, so it is withheld whenever any rule covers reading a
  path.

Two settings go further than any list of rules: `readsStayInWorkspace` refuses every path outside the
working directory, and `bypassUnreachable` takes the mode that asks about nothing out of reach. See
[Configuration](../customize/configuration.md#permissions).

A deny rule also holds against **a workspace you trusted**, which is what makes one worth writing:
saying yes at startup trusts the whole tree, and a rule is how one file is kept out of that answer
without declining the rest of it.

**An allow rule stops the asking and grants nothing else.** It does not make a command's output
trusted: output carries what it would have carried. Pressing `a` at [a run prompt](permissions.md#vouching-for-a-command) grants those two together because
you are looking at one command and can answer for both; a pattern covers commands nobody has read, so
it cannot carry the second claim. If a rule could trust output, one line in a settings file would turn
fetched bytes into routing, which is the whole thing labels exist to prevent. Nor does an allow rule
extend reach: it cannot open a path the workspace and the directories you opened do not already cover.

**An allow rule has to be yours.** `deny` and `ask` work from any settings file, including the
`.bravebot/settings.json` and `.bravebot/settings.local.json` that arrive with a repository, because
both of those only ever take something away. `allow` is the one list that hands something back, so it
is read from `~/.bravebot/settings.json`, and from a file you passed to `--settings` that sits outside
the project you are working in and the one you started in. An `allow` entry in a checkout's file takes effect on nothing by
itself: whoever last edited the repository would otherwise be answering an approval prompt on your
behalf, which is a program run with your privileges, a file written without the question, or a
redirect to a host you were never shown.

**A project asks, and you grant.** When a session opens in a checkout whose settings carry `allow`
entries, one box lists every one of them and the file each came from. Saying yes grants exactly those
rules; saying no grants none and the session carries on asking about each action as it would have.
The answer is kept per project, in `~/.bravebot/granted/`, so the next session there does not ask
again, and a project that edits a rule after you granted it asks about the new one. Delete the file,
or the line in it, to take a grant back.

A grant stays with the project it was given in. `/cd` into another checkout reads that checkout's
rules and puts its own box where it proposes something, so a rule you granted for the first one
answers nothing in the second, and the second's `deny` rules are in force from the first turn there.
A subdirectory has no rules of its own to read, so `/cd` into one leaves the checkout's behind, as
starting there would. `/clear` reads the rules again for the directory you are in.

`bravebot doctor` names every such rule and says which of the two it is, for the directory you run it
in, so neither the prompt you still get nor the one you no longer get is a mystery. Waiving a prompt
for *every* project is still your own file, or a file passed on the command line for one run.

**Five prompts no rule can answer.** A run that would put your private data into a program asks
whatever the rules say, because a rule saying which commands may run is not consent to hand one your
data. A call that would put your private data into an [MCP server's](../customize/mcp-servers.md#each-call)
tool asks for the same reason, and a standing answer to that server does not cover it. That is every
call once something private was let out of quarantine for the model. A write whose
destination is known only through a reference asks too: that prompt is the only
moment such a path is shown to anybody, so nothing a pattern says can stand in for having looked. And a
run carrying a variable set in front of one of its programs asks, because a rule is matched against the
program and its arguments: `Bash(git log)` matches `LD_PRELOAD=./evil.so git log`, and no rule you could
write tells the two apart. Last, a write to a [protected path](permission-modes.md)
asks, an `Edit` allow rule and a vouched path included, because a file there is code that runs later with
your own rights.

**A rule that cannot be read is dropped, named, and takes nothing with it.** A line that is not a rule,
names no tool family, or has no anchor to resolve is skipped and the rest of the file still applies.
Every one dropped is reported, both by `bravebot doctor` and in the session that read the file, because
a misspelled `deny` rule reads as protection that is not there. Refusing the whole file instead would
mean a typo in an `allow` rule quietly removed a `deny` rule's protection.

Rules are read **once per session**, so a file you edit while a session is open describes the next one.

:::note
**A `deny` list is not a sandbox.** A rule is matched against a program's argv rather than against
what the program then does, so `Bash(git *)` covers `git -c core.fsmonitor=<script> diff`, which runs
a program the rule never named. And a path rule does not reach a program's own file access at all:
`run cat .env` is checked against the `Bash` rules and against the run prompt, not against a `Read`
rule covering `.env`. What confines a process is [confinement](security.md#confinement); what holds
regardless is the label on the output.
:::
