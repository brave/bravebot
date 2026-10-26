---
sidebar_position: 2
title: Skills
description: Package a repeatable piece of know-how as a SKILL.md the planner loads when the task calls for it.
---

# Skills

A skill is instructions you wrote for a kind of task. Put one in
`~/.bravebot/skills/<name>/SKILL.md` and it is available in every project; put it in
`<workspace>/.bravebot/skills/<name>/SKILL.md` and it belongs to that project. A project's
`.claude/skills` and `.agents/skills` are read too, so a skill written for another agent works
here as it is.

```markdown
---
name: commit-style
description: How commit messages are written here. Use before writing one.
---

Write the subject in the imperative. Explain why in the body, never what.
```

## The file

One `SKILL.md`, with `name` and `description` in front matter. Both keys are required, and a file
missing either is skipped with a note saying so. `argument-hint` is drawn after the skill's name
in the box to say what it takes, as in `argument-hint: '[low|high] <pr#>'`, and is never shown to
the planner. A key nothing here reads stops nothing, so a skill written for another agent works
here; `bravebot doctor` lists such keys, so a line you expected to do something and that does
nothing is somewhere you can find it. A file with no front matter is not a skill.

A value may wrap over the lines indented beneath it, however the file spells the wrap: folded or
literal with `>` or `|`, quoted and carried over, or plain text continued. A folded value is
joined with spaces; a literal one keeps the newlines it asked for.

## Running a skill on its own model

Two more keys, both optional:

```yaml
name: release-notes
description: Turn a range of commits into release notes. Use when cutting a release.
model: haiku
effort: low
```

`model` is resolved the way the `model` settings key is, `opus`, `sonnet` and `haiku` included.
`effort` is one of `low`, `medium`, `high`, `xhigh` and `max`. Leave either out and the session's own
choice stands.

Both take effect from the moment the skill is loaded, for the rest of that turn, and **they replace
a model you chose for the session**, including one picked with `/model`. The session says so when
it happens, so you see when a skill moves a turn onto a dearer model. On a model of a ChatGPT account the
skill's `effort` changes nothing, since a session there keeps the level it began with, and the session
says so.

A skill does not replace a model that a definition named. When you address a definition that names
a model, or a delegate's definition names one, a skill loaded in that turn keeps the definition's
model and the session says so. The skill's `effort` still applies. Where `--model` on the command
line took the place of the definition's model, a skill's model replaces the command line's, as it
would with no definition addressed.

If the service answers a skill's rounds with a different model than the one the skill named, which
it does for a misspelt name or a premium model with no subscription in hand, the session says so.

A value that cannot be used is reported and the skill still loads, on whatever the session was
already running: a level spelled some other way, or a model needing a sign-in this machine has not
made.

## Only the name and description reach the prompt

The body waits until the planner asks for it with `load_skill`, so a directory of long skills does
not crowd out the task. The **description is what the planner decides from**. Write it to say
*when* to use the skill rather than what it contains:

```yaml
description: How commit messages are written here. Use before writing one.
```

not

```yaml
description: Notes about commits.
```

Only a skill built into bravebot itself, such as the one that paces a `/loop` tick, is listed by
default. A skill of your own or a project's is not: the planner finds it by calling
`search_skills` with a word or phrase, which matches it by name or description and returns it in
full, then loads it with `load_skill` the same way. The one exception is a definition's own
`skills:` line: naming skills there already makes a short, deliberate list, so those are listed
directly.

The first turn of a session that has one to name also says how many such skills there are and
what each is called, so the planner can tell whether searching could find anything before it
tries. Just the names, never a description: a description is what `search_skills` is for. Said
once, not on every turn, since repeating it would cost what listing them outright did.

A description nobody wrote to be long is still returned in full, never cut, however it is found.
Where one is long enough to be worth saying so, past 1,024 characters, the session tells you a
count of how many are, never which ones.

## Naming a skill after a slash

Type `/` and the skills this session has are listed beneath the commands, each with its description
and where it was found: `(project)`, `(user)` or `(built-in)`. After other words, as in
`this is /release-no`, the list holds skills alone. Tab takes the highlighted one and writes
`/release-notes ` into the line. A word that is already a command in full still lists the skills
whose names extend it, so `/pr` shows `pr-fix` as well as the `/pr` command.

A name typed in full is drawn in the prompt's colour, so you can see it was recognised. With a space
after it, the skill's `argument-hint` appears dimly where the argument goes, and goes as soon as you
type.

The line is still a prompt, sent as you typed it. The planner is told that a prompt naming a skill as
`/name` is you asking for it, and it loads the skill with `load_skill` the way it loads any other, so
a skill you named is loaded and recorded exactly as one it picked for itself. A skill that only you
can load, described [below](#a-skill-only-you-can-load), is the exception: the turn loads it for you.

The list is the set the planner is offered, plus any skill [only you can load](#a-skill-only-you-can-load):
a skill in a project you have not trusted is never listed, just as it is never shown to the planner. No skill is a slash command, so one whose
name a command already claims, such as `loop`, is not listed, and nothing is completed after a
command's own word or in [shell mode](../using/shell-mode.md).

## Keeping a skill applied

`/mode <skill>` keeps a skill applied to every turn until `/mode off`, instead of naming it in each
prompt. See [`/mode`](../reference/commands.md#mode-skill--off) for what it takes and how it is shown.
Its instructions are put in the system prompt each turn, so they are not repeated in the
conversation, and they are read again from the trusted source at the start of each turn.

## A skill only you can load

A skill for a step with side effects, such as a release or a deploy, should run only when you ask
for it. Add `disable-model-invocation: true` to its front matter:

```yaml
name: release
description: Tag the release and publish it.
disable-model-invocation: true
```

The planner is not told the skill exists. Neither its name nor its description is in the system
prompt, and a `load_skill` call for it is refused as it is for a name that matches nothing, even in
a turn where you named it. You load
it by naming it in the line you type, as in `tag and publish /release`. The turn then hands the
planner the skill's instructions after your line, with the names of the files beside it, and the
planner makes no call. The `model` and `effort` the skill names apply from the first request. The
session tells you the skill was loaded, since no tool call is drawn for it.

Only a word of your own line that is `/release` names the skill, with any quotes, brackets or sentence
punctuation around it, so `run /release.` and `run (/release)` work and `/Release`, `/releases` and
`release` do not. A task a delegate was given, a line bravebot composed itself and text inside a file
or a tool result never name one, and a delegate is never offered such a skill, even where its
definition's `skills` line names it.

`true`, in any case, keeps the skill from the planner, and `false` leaves it as it was. A trailing
`# comment` is ignored. Any other value is read as `true` and the session tells you, because a
misspelling that left a skill with side effects open to the planner would expose the skill the key
was written to protect. The list after a slash still shows the skill, marked `(only you)`, since it
is where you find the name to type.

A name works under an addressed definition whatever its `skills` line lists, and in a line you pasted
text into: the word is compared with the names, not read for what the sentence means. A name works in a
line you send as a prompt. One typed while a turn is running is read as words by
the planner and loads nothing, and a loop that names the skill supplies it again on every tick. For
a skill in `~/.bravebot/skills`, the planner can read the files beside it only in the turn that named
it, so a later turn that needs one asks you to name the skill again. A
word `/release` anywhere in the line names the skill, including a line that says not to run it, so
the instructions reach the planner with the words "Apply them as that line asks" and the session
tells you the skill was loaded.

If a long conversation is compacted after you named such a skill, the skill is sent again in its own
words, and a skill too long to be sent whole says that only you can have the rest loaded again.

## Loading

`load_skill` takes a name, and the name selects from the set found before the turn started. It is
never a path: a name holding `../` or an absolute path matches nothing and the call is refused, since
there is no lookup for it to reach. A name merely close to a real one is refused too rather than
guessed at, because guessing would load instructions nobody asked for.

## Files beside the skill

A skill is a directory, so it can keep a `reference.md` or a `scripts/` folder next to its
`SKILL.md` and say in the body when to read them. When the skill is loaded the result ends with the
directory and the names of the regular files beneath it, found before the turn started. The planner
reads one with `read_file`, naming the directory and the file.

For a skill in `~/.bravebot/skills` that read is allowed once the skill has been loaded, for the rest
of the turn, and the file is trusted as the skill is: it is your own directory. Nothing else there
opens up: a write is refused, a path that climbs out of the skill's directory or leaves it through a
link is refused, and a setting that keeps the file tools inside the project still does. A link that
stays inside the skill's directory still reads; one that lands in a directory you added with
`/add-dir` or in the session's scratch directory is refused, so a file from there is never read
under the skill's trust. A skill in a
project is read through the trust map like any other file of the project, so a file you have not
vouched for is not named, and one a `deny` rule covers is not named either. A link in the skill's
directory is never listed. Directories that are generated (`__pycache__`, `node_modules`) and compiled
`.pyc` files are left out of the list too, so a skill whose scripts have been run still names its real
files. Running a script kept there is not part of this; `run` has its own gate.

## Trust

| Source | Trusted because |
|---|---|
| `~/.bravebot/skills/<name>/SKILL.md` | it is your own directory: provenance, never the trust map |
| `<workspace>/.bravebot/skills/<name>/SKILL.md` | you vouched for the directory |
| `<workspace>/.claude/skills/<name>/SKILL.md` | you vouched for the directory |
| `<workspace>/.agents/skills/<name>/SKILL.md` | you vouched for the directory |

Each project skills directory is checked for trust **before it is enumerated at all**, because a
directory name is content too. A source that fails the gate is dropped entirely, and what was skipped
is counted rather than named. See [Instructions](instructions.md#trust).

Where the working directory is your home directory or a directory holding it, or you opened such a
directory with `/add-dir`, `~/.bravebot/skills` is inside the session's reach and a turn can write
to it. So it can where the way to a skill goes through the project, for example when
`~/.bravebot/skills` links into a dotfiles repository you are working in. Those skills are then not
read as your own. In a session started in your home directory they are read through the trust map
as `<workspace>/.bravebot/skills`. From a directory above it, they are read that way once the
session has read or written something under your home directory, as any directory's
`.bravebot/skills` is. Where nothing reads them, you are told how many were not loaded.

A project skill replaces a global one of the same name, and one in `.bravebot/skills` replaces one
of the same name in `.claude/skills` or `.agents/skills`.

A project skill that a `deny` [rule](configuration.md#permissions) covers, under the name it was
found by or the file it links to, is not offered, and you are told it was left out.

A few skills are written into bravebot itself rather than found on disk. They pass no trust gate and
are offered in every session, including one in a directory nobody trusts, because there is no file and
no directory behind them. They are the least specific source, so a skill of your own with the same
name shadows one. The skill that tells a
[`/loop`](../reference/commands.md#loop-interval-prompt) tick how to pace itself is one,
and `bravebot-help` is another: it carries these documentation pages, so asking how to use bravebot
is answered from the version you are running. Its pages are loaded one at a time, only the ones the
question needs. It also carries the heading of every clause of the specifications, one page for each
spec, for a question about exactly what bravebot does.
[`/help <question>`](../reference/commands.md#help-question) asks for it by name.

:::caution
A skill downloaded into `~/.bravebot/skills` is trusted exactly as far as a config file you pasted
is. The name, the description and the body all go to the model as instructions, and nothing
downstream second-guesses it, because everything downstream is built to trust what you vouched for.
Read one before installing it.
:::
