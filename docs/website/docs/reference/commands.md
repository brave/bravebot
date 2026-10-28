---
sidebar_position: 2
title: Slash commands
description: The thirty-one commands the interface acts on itself, and the rules every one of them shares.
---

# Slash commands

A line beginning with `/` is acted on by the interface itself, in place of being sent anywhere.

| Command | Argument | What it does |
|---|---|---|
| `/status` | | Report this session, what it may touch, and what it has spent |
| `/cost` | | Show what each turn of this session has spent |
| `/context` | | Show what fills the context window, by category |
| `/request` | | Show the last request sent to the model, and where each part of it came from |
| `/model` | | Choose which model to think with |
| `/theme` | `[name]` | Choose which theme paints the interface |
| `/effort` | `[level]` | Choose how hard to think before answering |
| `/config` | | Choose how the input box edits text |
| `/language` | `[tag \| auto]` | Choose the interface language for the next start, or drop the choice |
| `/add-dir` | `<path> \| close <path>` | Open another directory and trust it for this session, or close one |
| `/cd` | `<path>` | Work in another directory from now on, and trust it for this session |
| `/rename` | `<name>` | Call this conversation something else |
| `/note` | `<note>` | Note something on this session's record, without putting it in the conversation |
| `/advisor` | `[model \| off]` | Name the model the planner may consult, say which it may, or drop the choice |
| `/limit` | `[<amount> \| off]` | Show the session's spend limit, set it in tokens or credits, or remove it |
| `/style` | `[name \| off]` | Choose how the planner answers: list the styles, pick one, or clear the pick |
| `/mode` | `[skill \| off]` | Keep a skill applied to every turn: list skills, pick one, or leave |
| `/help` | `<question>` | Ask a question about bravebot, answered from its own documentation |
| `/changelog` | | Show what changed in this version |
| `/compact` | `[focus]` | Summarise the conversation so far, keeping the recent part |
| `/btw` | `<question>` | Ask something beside the work, without putting it in the conversation |
| `/recap` | | Recap where this session stands, without putting it in the conversation |
| `/clear` | | Start a new session here, keeping this one resumable |
| `/handoff` | `<next goal>` | Start a new session from a brief you can edit, written for the next goal |
| `/branch` | `[<name>]` | Copy this session and carry on in the copy, keeping the original to return to |
| `/resume` | `[<id>]` | Pick up another session of this directory, by id or from a list |
| `/bg` | | Hand this session to a background process that keeps running after the terminal closes |
| `/detach` | | Leave a background session you attached to, leaving it running |
| `/forget-trust` | | Stop remembering that this directory is trusted, so later sessions here ask |
| `/reach` | `[<where> -- <command>]` | Remember a directory or credential for a command, or list and remove them |
| `/sandbox` | `[<mode>]` | Show the sandbox mode, or change it from the next turn |
| `/loop` | `[[interval] <prompt> \| stop [n]]` | Send a prompt again and again, say what is repeating, or stop it |
| `/after` | `<delay> <prompt>` | Send a prompt once, after a delay |
| `/goal` | `[<condition> \| clear]` | Keep working until a condition you set is judged met |
| `/watch` | `[stop <n>]` | List the files this session is watching, and stop one by its number |
| `/jobs` | `[stop <name> [<delegate>]]` | List this turn's background jobs, and stop one by its name |
| `/outputs` | | Open what the commands this session ran printed, background jobs included. Alt-L does the same |
| `/panel` | | Show or hide the info panel beside the transcript |
| `/diff` | | List the files the session wrote and show every edit to the highlighted one |
| `/todos` | | Print the whole plan, every task with its status. Alt-A does the same |
| `/unfinished` | | List what this session left open, and whose move each is |
| `/caffeinate` | | Keep the computer awake while a turn or loop is pending |
| `/thinking` | | Show or hide the model's thinking as it arrives |
| `/pr` | `[<url> \| clear]` | Say which pull request this session is for, show it, or clear it |
| `/issue` | `[<url> \| clear]` | Say which issue this session is for, show it, or clear it |
| `/checkouts` | `[apply <n> \| remove <n>]` | List kept checkouts, bring their files back, or remove one |
| `/plan` | `[task]` | Enter plan mode, and start on a task if you give one |
| `/manifest` | `<task>` | Plan one task in full, show you the plan, then run it with nothing re-planned |
| `/agent` | `<name> <task>` | Run one of your definitions on a task, by its name |
| `/memory` | | List each definition's memory, where it is kept and whether it is withheld |
| `/mcp` | `[list \| reconnect [alias] \| disable <alias>]` | List the MCP servers, start one that is not running, or turn one off for the session |
| `/lsp` | | Say which language servers this session has started |
| `/open` | `[edit] [n \| address \| path]` | List the links in the latest reply, and open one by its number, or open an address or a file you type |
| `/init` | | Have the planner draft an `AGENTS.md` for this project |
| `/review` | `[target] [focus]` | Have the planner review local changes or a pull request |
| `/commit-message` | `[source] [format]` | Draft a commit message from the staged changes into the box, for you to edit |
| `/export` | `[path]` | Export the session transcript to a markdown file |
| `/copy` | `[n \| block [n] \| text [n]]` | Put a reply on the clipboard; block copies a code block, text the drawn words |
| `/undo` | | Rewind one turn and put back the files it wrote |
| `/rewind` | `[turns]` | List the turns a rewind could go back to, or go back that many |
| `/hunks` | `[number]` | List the hunks the last turn wrote, or put one back as it stood before the turn |
| `/exit` | | Leave |

Typing `/` offers the list in that order, with your [skills](#skills-after-a-slash) beneath it, and
Tab completes. The list is one row per command and per skill, and a terminal without the room for all
of them drops the last ones, skills first: everything is still typeable in full, and a letter or two
narrows the list back onto the screen, but a short terminal costs you the discovery the list is there
for.

A command typed in full is drawn in the prompt's colour, and once a space follows it, the argument
it takes appears dimly after the cursor until you start typing one.

`exit` and `quit` typed without the slash are prompts, not commands. A line that is only one of them
is sent at the first Enter like any other prompt, and while the turn it starts runs, the row under
the box says that it was sent as a prompt, that Ctrl-C stops it and that `/exit` leaves.

## `/status`

Reports everything the session knows about itself:

- the working directory, and anything opened with `/add-dir`, and whether a later session started
  there will trust it without asking because a yes was [kept](#forget-trust);
- under [bypass](../security/permission-modes.md#bypassing), each directory outside the workspace the file
  tools were taken into, with the tool that first reached it. Nothing is opened or trusted by it;
- each [checkout](../customize/agents.md#a-checkout-of-its-own) a delegate kept or that could not be
  removed, with its number, the commit it holds and the delegate it was made for, since the
  transcript says one was kept but not where;
- the model in force, and whether it was chosen or defaulted. Where the server substituted a
  different one, the model that actually answered is shown beside it;
- the [effort level](#effort-level), and whether this model reads one;
- which deployment the endpoint names, and **which tier the last turn ran on**, rather than which
  tier the build was compiled to reach;
- the confinement available here, and under `workspace-write` or `read-only` on a Linux kernel older
  than Landlock ABI 9 a `Programs' unix sockets` row saying that every socket with a path is
  reachable (D-Bus, `SSH_AUTH_SOCK`, `docker.sock`);
- turns and tokens spent, and **where the time went**: how much was spent waiting on the model,
  running tools, and waiting for you to answer a prompt;
- each [background job](run-tool.md#leaving-a-pipeline-running) of the last turn: its name, and the
  delegate's number where a delegate started it, its line cut short, how it ended, and whether it was
  started in the background or you moved it there. Typed while a turn runs, `/status` answers at
  once and lists the jobs the turn has started so far;
- **every trust rule in force**, listed in full, each marked trusted or untrusted;
- **every command you vouched for**, which now run unasked and whose output is read as trusted.
  Typed while a turn runs, these two lines say the running turn holds them and show them once it
  ends, since the turn can add to both as it goes;
- the [`/reach`](#reach) grants remembered for this workspace, numbered as `/reach` lists them;
- what a [`/loop`](#loop-interval-prompt) is repeating and when the next tick is due, where one is
  running, or what a [`/goal`](#goal-condition) is working towards, how many rounds it has
  spent, how long it has been set and what the session has spent since.

The last three are the ones nothing else on your screen tells you. A vouched command is the one that
stops appearing, and what happens next without anybody typing anything cannot be read off the
transcript.

The endpoint host and the key id are left out, though `bravebot doctor` prints both. A status panel
is the thing people paste into an issue or a screenshot.

## `/request`

Opens a read-only view of the last request built for the model: the system prompt in the pieces it was
put together from, each message, and each tool result as the model saw it. Every part is headed with
where its words came from:

| Label | Words from |
|---|---|
| `typed` | what you typed |
| `trusted file <path>` | a file you named or vouched for |
| `tool result (trusted)` | a result the kernel let the model read |
| `ref:N` | content the model was not shown. You see the reference token it saw, never the content |
| `driver` | a sentence bravebot wrote |
| `planner` | the model's own earlier words |
| `unrecorded` | nothing recorded it, as for a message restored from a saved session |

The view is read from the request that was sent, not rebuilt from the transcript, so it holds nothing
the model did not. Above the spans it names each instruction file the system prompt was built from, with
the bytes it contributed, or says none loaded. It writes nothing to disk, and an incognito session is unchanged. Typed while a
turn runs it answers at once; It is the transcript scroller, so it scrolls and closes as that does. The view is of the main conversation, not of a
delegate's. It takes no argument: `/request` followed by words is a prompt.

## `/context`

Reports what the last request was made of, as the count the server gave for it shared out over
fixed sections: the system prompt, instruction files, the skill list, tool definitions, what you
typed, what the planner wrote, tool results, and other messages. A section with nothing in it has no
row. Typed while a turn runs, it answers at once.

The figures are scaled from the bytes of each section to the measured count, so they add up to it and
are approximate. Only the last request is broken down, since the cache split and sizes are not kept
per turn. Before anything is measured it says so, and after a compaction it says the conversation was
compacted. It takes no argument: `/context` followed by words is a prompt.

## `/lsp`

Says which language servers the session has started: how many, and each by the program you approved,
in name order, with a note that it runs with your own access until the session ends. A server starts
the first time the model asks about a file it has a server for, after you approve it, so a session
that has asked nothing about code has none, and `/lsp` says so. It lists the same programs as the
Language servers section of the [info panel](../using/sessions.md#telling-sessions-apart), which is
where to look when the panel is open, and clicking one of its rows says the same. Typed while a turn
runs, it answers at once. It takes no argument: `/lsp` followed by words is a prompt.

## `/cost`

Reports what the session has spent as a total, and under it **one figure per turn with that turn's
share of the total beside it**. What was spent before the first turn is reported too, without a turn
number, since no turn did it. Typed while a turn runs, it answers at once, and the running turn's row
holds what it has spent so far.

A total cannot tell twenty even turns from one that ran away, and the share is what makes the second
one visible without your dividing each row by the total. It is a word of its own rather than more rows
on [`/status`](#status) because the list grows with the session: a panel that answers "what is this
session" in fifteen rows would answer it in fifty on the fiftieth turn.

:::caution
**The figures are tokens, and they are not a bill.** A prompt a service answered out of its own cache
is charged at a fraction of a fresh one, so two turns recorded at the same figure can differ about
tenfold in money. The breakdown keeps no cache split per turn, so a figure in money would be composed
here rather than measured. What is written stays comparable across turns and across sessions, which is
what it is read for.
:::

## `/model`

Opens a picker on the model in use. The list comes from the endpoint rather than a set compiled in, so
it is whatever the backend offers today. Google Vertex AI, which has no listing to ask, is the
exception: it is offered a short Gemini list built in. The choice is written to `~/.bravebot`, so it
outlives the session and applies in every directory, except one whose own settings name a
[`model`](../customize/configuration.md#model): that key outranks the choice, and the one in your own
`~/.bravebot/settings.json` does not.

Typing narrows the list rather than walking it, and rows are grouped under the service that answers
them. See [Choosing a model](../customize/model.md).

In a session started with [`--agent`](cli.md#--agent-name) under a definition that names a model,
every turn uses that model, so `/model` opens no picker and says so.

## `/theme [name]`

Opens a picker on the palette in force. With a name, `/theme nord` applies it without opening the
panel. Typed while a turn runs it is applied at once, ahead of anything waiting, and the bare `/theme`
opens its picker over the turn.

Up and Down move the cursor, and the theme under it is put in force while it is selected, so you are
comparing themes against your own transcript rather than against a sample. Enter keeps the one on the
cursor and Escape restores the one that was in force when the picker opened.

The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory.
Themes of your own are JSON files under `~/.bravebot/themes/`, and nothing in a workspace is read. See
[Choosing a theme](../customize/theme.md) and
[Themes](../using/transcript.md#themes).

## `/advisor [model | off]`

Names the model the planner may consult through the [`advisor`](tools.md#advisor) tool, from the
next turn on. `/advisor opus` takes it, in the words [`--advisor`](cli.md#--advisor-name) takes, and
it outranks the [`advisorModel`](../customize/configuration.md#advisormodel) setting. The bare
`/advisor` says which advisor is in force and opens no picker. A model that this machine's
administrator refuses, that nothing configured serves, or that needs a sign-in first is not taken,
and the line says why.

`/advisor off` drops the choice made here. It does not switch off an advisor the setting names, and
says so when the setting still names one. The choice lasts for the session and is not written down:
put the model in the setting to have it in every session. Each call the planner makes spends the
advisor's tokens and is counted in the turn's.

Typed while a turn runs with nothing waiting, it is taken at once and the next turn is the first
to use it.

## `/limit [<amount> | off]`

Sets the most tokens, or Leo Premium credits, this session may spend. `/limit 500k` or `/limit 2m`
sets it in tokens, with `k` for thousands and `m` for millions, `/limit 200 credits` sets it in
credits, `/limit off` removes it, and `/limit` alone says what it is and what
has been spent. The `limit` key in [the settings file](../customize/configuration.md#limit) starts a
session under one.

When the session has spent as many tokens or credits as the limit, the next request is not sent. You are asked
whether to stop, to go on without a limit, or to go on under a new one: choose the last by answering
in your own words with a figure in the same unit above what is spent, such as `2m` or `300`. Nothing else answers for you, so
the question appears in every permission mode, including the one that asks about nothing. A
`/loop` or `/goal` run ends there. Tokens are the usage the model service reports, so a limit in
tokens is not money, and a service that reports no usage is never stopped by it. Credits are the
credentials this session presented, one per premium request, delegates included. A session with no
subscription spends none and is never stopped by a limit in credits, and `/compact`, `/aside` and
goal checks are not counted. `/clear` starts the count again, and a resumed session starts at zero. The round that reaches the
limit has already been paid for, so the total can end a little above it.

Typed while a turn runs it is taken at once, and the running turn's next request is held to it.

## `/style [name | off]`

Sets how the planner answers, from the next turn on. Three styles ship with bravebot: `concise`
leads with the result and leaves out preamble and recap, `explanatory` adds short `Insight` notes on
why the code is the way it is, and `proactive` starts work and decides routine questions itself,
saying what it assumed. `/style` alone says which is in force and lists the names, and `/style off`
clears the pick.

A style stands where [`--system-prompt`](cli.md#--system-prompt-prompt-and---append-system-prompt-prompt) stands: it replaces the
opening of the system prompt and nothing after it. If you also gave `--system-prompt`, that is used
and the style is not. A style grants nothing: writes are still put to you where they would have been,
and plan mode still refuses them. The pick lasts for the session and is not written down.

Your own styles are files in `~/.bravebot/styles`: `terse.md` there is the style `terse`, and its
words replace the opening in the same way. A name is lowercase letters and digits joined by hyphens.
A file that is empty, longer than 4096 bytes or a symbolic link is not offered, and a file named
after a built-in style is ignored.

A project can hold styles too, as files in `.bravebot/styles` under the workspace, with the same rules
for the name and the file. They are read only where you trusted the project, the way its `AGENTS.md`
is, and are listed after the others when you type `/style`. A project's file cannot take the place of
a built-in style or one of your own. The file is read again at the start of each turn, so a style
whose file is no longer trusted is not used and the turn says so. The `style` setting does not
name a project's style.

## `/mode [skill | off]`

Keeps one skill applied to every turn until you leave it, for a skill that says how to work, such as
a review checklist or a test-first playbook. `/mode` alone says which skill is the mode and lists
the names it could take, `/mode tdd-playbook` puts that skill in force from the next turn, and
`/mode off` leaves it. A name is compared whole against the skills the planner would be offered, plus
[the ones only you can load](../customize/skills.md#a-skill-only-you-can-load): a prefix, a different
case or a skill from a project you have not trusted is not taken, and the mode you had stays.

The skill's instructions go in the system prompt of each turn, after bravebot's own words and ahead of
your `AGENTS.md`, so your own instructions still have the last word. They are not added to the
conversation, so a long session holds them once. The skill is read again at the start of each turn, so
one that was edited is followed, and one that is gone or no longer trusted is not applied: the turn
says so and goes on without it. Only the instructions apply; the skill's `model` and `effort` do not.

The mode's name is shown in the bottom border of the box while it is in force, it is written to the
session's record, and a resumed session is back in it and says so. A delegate is not given it.
Typed while a turn runs, `/mode` waits for the turn to end.

## `/effort [level]`

Opens a picker of the five levels (`low`, `medium`, `high`, `xhigh` and `max`) above a row for
asking for no level at all, so a first pick is not permanent. With a word, `/effort high` takes it
directly. Tab finishes a started level, so `/effort h` becomes `/effort high`, and leaves a start that fits more than one level, such as `m`, as typed. A word that names no level changes nothing and says so rather than reaching a request
field. Typed while a turn runs with nothing waiting, `/effort high` is taken at once and the next turn
is the first sent at it, since the running one keeps the level it began with. On a model of a ChatGPT
account the level is fixed for the whole session instead: the account is sent the level the session began
with, and the choice applies from the next session. The bare `/effort` waits for the turn to end.

The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory,
except one whose own settings name an [`effort`](../customize/configuration.md#effort): that key
outranks the choice, and the one in your own `~/.bravebot/settings.json` answers only while no level
is recorded. Picking the row for none removes the record, so from the next session your own file's
level answers again. See
[Choosing how hard to think](../customize/effort.md), which is also
where the two cases worth knowing are: the models that read no level, and the Brave endpoint, which
accepts one and discards it.

## `/config`

Opens a panel over the transcript for a preference about the interface. It lists the choices with
what each one means and marks the one in force. Enter takes the row under the cursor and says so on
the transcript; Escape leaves the setting alone, which is what makes the panel safe to open just to
see what is set.

It holds one choice today: whether the input box edits the ordinary way or
[vi's](../using/interactive-mode.md#editing-the-way-vi-does). The choice reaches the transcript
because it changes what the next keystroke does and the box gives no other sign until a letter has
gone somewhere unexpected. Whichever style you choose, the box comes back taking letters as letters.

The choice is written to `~/.bravebot`, so it outlives the session and applies in every directory.
See [`editorMode`](../customize/configuration.md#editormode) for the settings key that answers for
somebody who has never used this panel.

## `/language [tag | auto]`

Says which language the interface is in and which catalogs there are, and names the choice saved with
this command where there is one. With a tag, `/language fr` saves it for the next start. The interface
is in one language for a whole session, so nothing on the screen changes until then. The tag saved is
the one of the catalog that answered, so `fr_FR.UTF-8` is saved as `fr`. A tag that no catalog answers
changes nothing and says so, with the catalogs there are. `/language auto` removes the saved choice, so
the `language` key of your settings file and then your shell answer again. Typed while a turn runs, it is
carried out at once.

The choice is written to `~/.bravebot/language`, so it applies in every directory. A session that records
nothing, an incognito one for example, says the choice was not saved and keeps none. See
[Choosing a language](../customize/language.md) for the order the sources are
read in, and for `BRAVEBOT_LOCALE`, which answers over this choice.

## `/add-dir <path>`

Makes a directory both reachable and trusted, for this session. `--resume` carries both halves and
`/clear` closes it. A directory already inside the project is refused. See
[Trusted directories](../security/trust.md#add-dir).

`/add-dir close <path>` closes that one directory and keeps the conversation. A file only it reached
is refused again, and the directory is no longer trusted. Every other open directory stays open.
`/status` lists what is open, under the names `close` takes.

An added directory contributes **no** standing instructions and no skills, whatever it contains.

## `/cd <path>`

Moves the working directory. From then on that is what a relative path means, where a program runs,
where `AGENTS.md` and the project's skills are looked for, and what `@` completes against. The path is
taken against where the session is now, so `..` and a name inside the project both work, and the
directory is trusted for the session on the same terms `/add-dir` grants.

The directory you left closes, and so does anything `/add-dir` had opened that holds the new working
directory or sits inside it. Each is said out loud as it happens, with the line that opens it again.
Nothing may overlap the working directory, because a file reachable both relatively and by absolute
path would have a rule in each namespace and so two answers.

Your trust map comes with you rather than being carried over unchanged. See
[Moving the working directory](../security/trust.md#moving-the-working-directory) for what happens to
each rule, and [Sessions](../using/sessions.md#a-session-that-changed-directory) for where the record
goes.

Your permission rules do not come with you. The new directory's rules are read, and anything its
checkout proposes is put to you in the box a session opening there would show, so a rule you granted
for the directory you left answers nothing here. See
[Permission rules](../security/permission-rules.md).

Nor do your answers about reading a file that holds what looks like a credential. Each is kept under
the file's name from the directory you left, which here can name another file, so such a file is asked
about again.

## `/forget-trust`

Takes back the yes that was kept at the question a session asks about its directory, whether given
here or in the desktop app, which keep it in one place. The next session started in this directory
asks again, in either. This session keeps the answer it already has;
[`/clear`](#clear) starts one that asks. Typed while a turn runs with nothing waiting, it is carried
out at once, and the turn goes on under the answer it already has.

It removes every answer kept about the path, including one given about a directory that was deleted
and made again there. In an incognito session it changes nothing, since nothing is written there
either, and it names the file so you can remove it yourself. See
[Remembering the answer](../security/trust.md#remembering-the-answer).

## `/reach`

Gives one command a place or a credential it needs, and remembers that, so the next plan for the
command carries it and the same refusal is not met in every session.

```
/reach docker -- docker build .             # docker's credential directory, for this session
/reach ~/cache write always -- make check   # make may read and write ~/cache, in every session
/reach                                      # list what is remembered, numbered
/reach remove 2                             # take the second one away
```

The first word is a credential scope (`remote`, `aws`, `kubernetes` or `docker`) or a directory.
A directory is read, and written only with `write`. It lasts this session, and a resumed one,
unless you add `always`. It is for the directory you typed it in, the workspace root, and applies
only there: after `/cd` into another checkout, or in a new session started in one, the row is not
in force, and `always` means always in that directory. The exception is a credential scope for a
program that scope belongs to (`git` and `gh` for `remote`, `aws`, `kubectl`, `docker`), which
applies everywhere. `/reach aws -- make check` is not that, so it is for this directory. Each program of the line gets it, on its own: a grant for `git push` is not
one for `git pull`, and a grant for one `make` is not one for another. A program started with a
`NAME=value` in front of it gets none.

The reach is shown with the command in the plan you are asked to approve, with the day you
allowed it. A directory must exist and is refused if it is your home or above it, or a place that holds a credential (such as `~/.ssh`, `~/.aws`, `~/.gnupg`, a browser profile or `~/.bravebot`) or inside one, and it is checked again each time it is used. A credential scope is never writable. What
a program printed when it was refused is never read for a path.

Typed while a turn runs, it waits for the turn to end. In an incognito session it reads what is
there and adds nothing, and says so. The record is `reach.jsonl` in `~/.bravebot`.

### Remembering a scope a `run` asked for

When the planner asks a `run` for a credential scope by name and the question is drawn, `m` runs the
line and remembers each scope it asked for, read only, for the programs of the line, for this
session. `e` does the same until it is removed, in this checkout or, for a program the scope belongs to such as `git` for remote, in every checkout. It is the row `/reach` would
have made, listed by `/reach` and taken away by `/reach remove`. The line is still asked about every
time, with the scope shown on it, and neither key trusts what it prints. A toolchain list the
planner asked for is not remembered, and neither is the `signing` scope, `loopback` or `ssh-key`, which are asked about every time. A line with a `NAME=value` in front of a program, or a
program started with an option such as `git -C dir push`, is not offered either key. The desktop app
does not offer them.

### The ssh key chosen for an owner

Where a `git` command names an ssh address of an owner no `git.identities` entry names, and ssh could offer
more than one of your keys, the run prompt numbers them and a digit chooses one, or `0` leaves the
key to ssh ([configuration](../customize/configuration.md#gitidentities)). The choice is kept in the
same file and decides later `git` commands that reach that owner.

```
/reach keys                                 # list the key chosen for each owner, numbered
/reach keys remove 1                        # forget the first, so the next prompt asks again
```

## `/sandbox [strict | read-only | workspace-write | off]`

Bare, it says the sandbox mode programs `run` starts are held to and what chose it: the default, a
settings file, the managed file, `--sandbox`, or an earlier `/sandbox`. With a word it sets the mode
for the rest of the session, starting with the next turn. Typed while a turn runs with nothing
waiting, a mode is set at once, `off` included, and the turn running keeps the mode it began with, so one turn's `run`
description, refusals and profile all come from one mode. The mode is
not written to the session, so a resumed session takes the mode the process started with, and `/resume`
inside a session keeps the one you set.

Once `/sandbox ` is typed the box lists the four modes after it, and Tab finishes a started one, so `/sandbox w` becomes `/sandbox workspace-write`.

Moving to `off` is made when you type it, with no question, and programs then start with no profile at all; the
reply says so. A mode looser than the managed file's `sandbox.mode`, or `off` under a managed `run.network` of
`closed`, is refused with a sentence naming that file. For a pinned `sandbox.mode`, the sentence says
whether the mode you asked for reads more of the machine, writes more, or both. `strict` and
`read-only` are each looser than the other in one of those ways, and a pin of `read-only` has no
stricter mode, so only the pinned mode is accepted. A managed `run.network` that is neither `open`
nor `closed` is read as `closed`, so it refuses `off` the same way. Otherwise the command ranks as `--sandbox`
does, so `/sandbox workspace-write` replaces a checkout's `strict`. Only you can set it: no tool, skill, hook
or `AGENTS.md` line can. The opening screen and `/status` name the mode in force. The desktop app has
no such command yet.

## `/after <delay> <prompt>`

Sends one prompt once, after a delay, and then forgets it.

```
/after 45m check whether the integration tests passed
```

The delay is the first word: a number and `s`, `m`, `h` or `d`, from five seconds to seven days. The
rest of the line is sent unchanged, as a prompt, when the delay is up and the session is idle. It
takes the place of a loop, so `/loop stop`, Ctrl-C, `/clear` and leaving end it, `/loop` says what is
pending, and it is not kept in the session record.

## `/loop [interval] <prompt>`

Sends a prompt again and again until you stop it, says what is repeating, or ends one. A session
holds up to five loops at once, numbered in the order they began.

```
/loop 5m check the deploy          # now, and every five minutes
/loop check the deploy every 20m   # the same, written the other way round
/loop watch the build              # now, and each turn says when the next is due
/loop                              # what is repeating, each loop under its number
/loop stop                         # end the loop, when only one is running
/loop stop 2                       # end loop 2 and leave the others
```

A second `/loop` starts another loop and leaves the first running. A sixth is refused. Ticks go one
at a time, whichever is due first, and each is announced with its loop's number. `/loop stop` with
several running ends none of them and asks for a number. Ctrl-C, stopping a turn, `/clear` and
setting a goal end every loop.

`stop`, alone or with a number, is the ending whenever it is the whole of the line that would be sent,
whatever its case and with or without a pace, so `/loop STOP`, `/loop 5m stop` and `/loop stop 2`
end a loop too. A line that only begins
with the word is a line: `/loop stop the deploy` is a loop over `stop the deploy`, and
`/loop stop the deploy every 20m` is that line every twenty minutes. The bare command with no loop
running says so, and so does `/loop stop`.

Typed while a turn is running, `/loop stop` and the bare `/loop` are carried out as you type them,
ahead of anything waiting in the queue. The turn in flight finishes, and no tick goes out after it.
A new loop typed mid-turn waits for the turn to end, and a `/loop stop` typed after it waits too, so
it stops that loop after its first tick.

The first tick goes at once, so you can see it happen while you are still watching. The gap is
measured from the end of a tick rather than its start, so `every 5m` means five minutes between runs.
A due tick waits for an idle session and never interrupts, and a prompt you type in the middle of a
loop is not a tick of it.

An interval is read off the front of the argument, or off an `every` clause at the end, in that order
and nowhere else. A leading token counts only when it is a number and one of `s`, `m`, `h` or `d`. A
trailing clause counts only when `every` is a word of its own and a time expression is the whole of
what follows it. Given no interval, each turn says when the next tick is due.

| The argument | The interval | The prompt |
|---|---|---|
| `5m check the deploy` | 5 minutes | `check the deploy` |
| `check the deploy every 20m` | 20 minutes | `check the deploy` |
| `check the deploy every 20 minutes` | 20 minutes | `check the deploy` |
| `check every PR` | none, so each turn paces it | `check every PR` |
| `check everything 20m` | none, so each turn paces it | `check everything 20m` |
| `5m check the deploy every 20m` | 5 minutes | `check the deploy every 20m` |

That is what keeps `/loop check every PR` a sentence rather than one with its last two words taken
off, and a word that merely begins with those five letters, such as `everything`, is not the clause at
all.

**The line a loop repeats is the one you typed.** It is settled the moment you press Enter and sent
unchanged for the life of the loop: nothing a turn reads, writes or returns can add to it, edit it or
replace it. A turn that could write its own next prompt would be rewriting its own instructions, and
the point of a loop is that it asks the same question again.

**A tick is a prompt, never a command.** `/loop 5m /status` sends the seven characters `/status` to
the planner every five minutes; it does not run the status command. A command is dispatched from a key
press, and a timer is not one.

| The wait | Shortest | Longest |
|---|---|---|
| an interval you gave | 5 seconds | 7 days |
| a delay a turn asked for | 1 minute | 1 hour |

A number outside those becomes the nearer bound, and you are told what it became rather than left
believing you are watching something ten times more closely than you are. A turn's number is held far
more tightly than yours because a turn that wants longer than an hour can say so in its answer, where
somebody reads it. Where you gave an interval, no turn can change it; a self-paced tick that says
nothing is woken once more twenty minutes later, and a second silence ends the loop. A tick that has
finished says so with `stop` and is not woken again.

Each tick is announced with its number, and with how many in a row have reported finding nothing.
That count is the difference between a loop that is working and a loop with nothing to do. Between
ticks the row under the input box says `looping, next in 4m`, since the note that announced the last
tick scrolls away and a loop spending a turn every five minutes is otherwise invisible; on a terminal
too narrow for everything that row carries, the loop is the last part given up before the permission
mode. `/loop` and `/status` both answer for it whenever you ask.

Six things end a loop, and each says so:

| What | When |
|---|---|
| you ask | `/loop stop`, or `/loop stop <number>` where several are live, which ends the loop and leaves the turn in flight running |
| you interrupt | Ctrl-C, reached after the turn in flight and the half-typed line, and before leaving; it ends every loop |
| a turn is stopped | any turn cancelled while a loop runs, tick or not; it ends every loop |
| the turn says it is finished | a self-paced tick that calls `schedule_next` with `stop`, which ends the loop with no further tick |
| the session moves on | `/clear`, and leaving, which end every loop; so does setting a goal |
| age | seven days after it started |

**A loop, a [goal](#goal-condition) and a [watch](#watch-stop-n) are never live together**, because a
session does one of the three at a time. Several loops may be live together, since the rule is about kinds. Whichever of a loop and a goal was asked for second stands,
and the one it replaced is reported as stopped. Asking for either ends every live watch.

**A loop is never written down.** It is not in the session record, so `--resume` restores none and it
does not outlive the process. A schedule that survived the session that set it would start sending
prompts at somebody who opened a conversation only to read it.

:::caution
**A loop keeps spending.** Every tick is a turn with the whole conversation re-sent, and nothing bounds
the total but the interval and the session's own life. A five-minute loop left open overnight is a
hundred and fifty turns nobody read. `/loop stop` ends one.
:::

## `/goal <condition>`

Keeps the session working until a condition you wrote is judged met. When a turn ends the condition
is put to a judge, and where it is not met yet the work goes back for another turn with the reason.

A one-shot run takes the same condition with [`--goal`](cli.md#--goal-condition).

```
/goal cargo test exits 0 and the diff is committed
/goal                              # say what the condition is, and how it is going
/goal pause                        # keep it, and stop judging turns against it
/goal resume                       # judge them again, against the same condition
/goal clear                        # take it off
```

`pause`, `resume` and `clear` act only when they are the whole argument, so `/goal clear the build directory and
the tests pass` is a condition like any other.

**Pausing keeps the goal for a detour.** While it is paused no turn is told the condition and none is
judged against it, so you can ask something unrelated or work by hand without losing the sentence.
`/goal resume` arms the same condition with the rounds it had spent, and `/status` and the panel say
it is paused. A check already out when you pause is not acted on. `/goal clear`, `/clear` and `/loop`
still end a paused goal, and it is not written down either.

**The report carries the cost.** `/goal` and `/status` say how long the goal has been set and the
tokens the session has spent since, and a goal that is met, cannot be met or gives up says the same
as it ends. Tokens are counted as each turn ends, so a turn still running is not in the figure yet.

**Setting a goal sends nothing.** A condition is not a prompt, so the session sits idle until you
ask for something; what a goal does is keep that work going. Nothing here writes a first prompt for
you, because there is no line you endorsed to send.

**The condition is the one you typed.** It is settled the moment you press Enter, and nothing a turn
reads, writes or returns can add to it, edit it or replace it. Only another `/goal` changes it. The
condition is what decides when the session is allowed to stop, and a turn that could write its own
would be deciding when it has finished.

The check is one request with no tools over a copy of the conversation, the shape
[`/btw`](#btw-question) uses, so the conversation the next turn resumes is the one that was already
there. Nothing the judge said arrives as your words either: what carries the work on is a sentence
written by this program, naming the condition and quoting the reason inside it.

**The judge is told what ran.** Beside the conversation, the judge reads this program's own count of the turns since the goal was set: how many commands ran, how many writes were made, and whether a command ran after the last write. Compaction can shrink the run that showed the tests passing to a sentence, and the count survives it. It says nothing about whether a command succeeded, and the judge still decides.

One answer carries the work on, and everything else ends the goal:

| What came back | What happens |
|---|---|
| the condition is not met yet | another turn, with the reason |
| the condition is met | the goal is over, and the reason is what you are shown |
| the condition can never be met | the goal is over, and the reason says why |
| an answer that is not one of those | the goal is over |
| the check itself failed | the goal is over |

A verdict is the first line of the answer and one of three words. Prose is not a verdict: reading
one out of a sentence nobody constrained would let the judge's wording decide whether your session
keeps working, and a sentence saying the condition is nearly met would read as either answer
depending on which words were searched for.

**Ten rounds and it gives up.** The last reason is kept, so a goal that has given up can still say
what it kept hearing. Three rounds in a row in which the turn made no tool call stop the goal from
sending work back, and it says so. The goal stays set, so your next prompt is judged against it, and a
round that does call a tool restarts the count. Four things end one besides a verdict, and each says so:

| What | When |
|---|---|
| you ask | `/goal clear` |
| you interrupt | Ctrl-C, reached after the turn in flight and the half-typed line, and before leaving |
| the session moves on | `/clear`, and leaving |
| the rounds run out | the tenth |

**Stopping a turn leaves the condition set.** Neither a turn you cancelled nor one that failed is
judged: a request that never came back says nothing about whether the work is finished, and an
interrupted turn says only that you did not want that turn. The goal stays set, and what is judged is
the next turn there is something to judge.

That is what makes a goal steerable. Stop the turn, say something else, and the condition is still
there; the press that ends the goal is the one you make with nothing running.

**A queued command that ends the goal stops the judging.** While a queued `/exit`, `/clear` or `/goal`
waits for the turn to end, the goal is not judged, so no more rounds are sent before it takes effect.
Any other queued command leaves the goal judged as before.

Escape and Ctrl-C take the goal off while a check is in flight, stop the check, and nothing more is
sent. With vi editing, an Escape from INSERT mode enters NORMAL mode first, as it does while a turn
runs. A verdict about a goal you have just taken off is neither acted on nor reported.

**A goal is never written down.** It is not in the session record, so `--resume` restores none and it
does not outlive the process. A condition judged against yesterday's conversation would start
working a session somebody opened only to read.

**A condition only somebody else can satisfy is waited for inside the turn.** Where the work is a
file you have yet to write, the turn sleeps and looks again rather than answering to be sent back:
one round of the ten costs a whole turn plus a judge's reading of the conversation, and a wait costs
a command. A command is killed at its deadline, which is at most ten minutes, so a longer wait is
repeated sleeps, and a condition hours away is not what a goal is for.

:::caution
**The judge reads the transcript, not the world.** It cannot run a command or open a file, so a
condition holds when the conversation shows it being observed: a turn that fixes something and never
checks the fix is sent back for not checking it. A condition no transcript could show, `the code is
clean`, spends all ten rounds and gives up, and nothing warns you in advance. Every round re-sends
the whole conversation, so ten rounds of a long session cost more than ten ordinary turns.
:::

## `/watch [stop <n>]`

Lists the files this session is watching, and ends one by its number.

```
/watch             # what is live, numbered
/watch stop 2      # end that one, leaving the others
```

**It arms none.** A watch is asked for in a prompt, and what a command is needed for is the half you
cannot read off the transcript: which watches are live, and how to end one. See
[Watches](../using/watches.md) for what a watch observes and what a firing puts in the conversation.

| Bound | Value |
|---|---|
| a watch's age | 7 days, after which it ends itself and says so |
| live watches in one session | 8, and arming a ninth is refused rather than dropping one |
| between two fires of the same watch | 5 seconds, measured from the end of the turn the last fire started |
| between two looks at a watched path | at most 5 seconds, which is what a firing's latency is |

Seven things end a watch and each says so: `/watch stop <n>`, Ctrl-C with nothing nearer to stop
(which ends every live watch), a firing's turn being stopped, asking for a
[`/loop`](#loop-interval-prompt) or a [`/goal`](#goal-condition) (a session does one of the three at a
time), the path ceasing to be readable, `/clear` and leaving, and age.

**A watch is never written down**, so `--resume` restores none and none outlives the process.

## `/jobs [stop <name> [<delegate>]]`

Lists the turn's [background jobs](run-tool.md#leaving-a-pipeline-running), and stops one by its name.

```
/jobs                 # each job: its name, its command, and how it stands
/jobs stop job:1      # stop the turn's job:1, leaving the others running
/jobs stop 1 d2       # stop job:1 of delegate d2
```

The list is the jobs of the turn running, or of the last turn when none is, with any job an earlier
turn left running, since a terminal session keeps its jobs until it ends. Each delegate numbers its own jobs from 1, so a delegate's job takes the delegate's
number after the name, and a name alone is the turn's own job. The list names a delegate's job the
same way, as `job:1 d2`, and says which delegate started it.

**The turn does the stopping.** The job is stopped at the turn's next step: when the round the model
is in ends, or at once if the model is waiting on that job's output. A wait on another job's output
ends then too, and the job is stopped at the next round. A reply the model is writing, a command run
in the foreground and a wait on a delegate are not cut short, so the stop comes after them. Its row
says `being stopped` until then. The model is told you stopped it and after how long, gets what it
had printed by then, and is told not to start it again unless you ask.
The rest of the turn goes on. The stop is recorded in the session's trail as yours.

`/jobs` is carried out as you type it while a turn runs, since a stop that waited would find nothing
left to stop. Between turns nothing reads the flag, so a stop kills the job at once and the model is
told at the next turn's first round that you stopped it. A stop still waiting when a turn ends, as when
the model's last reply made no tool call, is carried out as the turn finishes. If a row still reads
`being stopped` with no turn running, typing `/jobs stop` for it again carries the stop out. A kept
job you stopped between turns keeps its own row in the `/outputs` list, and does not appear a
second time as a plain command row.

## `/outputs`

Opens what the commands this session ran printed, background jobs included, one row each in the order
they ran. Alt-L does the same and is one of the [chords you can move](../customize/configuration.md#keybindings). Over the commands, Alt-L closes the view, and over the delegates it switches to the commands. It is the same mode [Ctrl-L](../using/interactive-mode.md#watching-a-delegate-reading-a-command-and-asking-something-aside)
opens over delegates and asides, with the same keys. Each row opens on what the command printed, says
how the run ended and whether the model read it, and a running job's row shows the lines it has
printed since the model last looked. Where no command has run, it says so and opens nothing. Nothing
it shows is sent to the model or written to the session's record.

## `/panel`

Shows or hides the info panel on the right of the screen, as Ctrl-X does. The panel holds the
session's name, directory and branch, the goal, how full the context is with the cache figures, and
the plan. It needs a terminal at least 100 columns wide, and says so on a narrower one. Whether it is
open is kept for the next session. [Telling sessions apart](../using/sessions.md#telling-sessions-apart)
has the rest.

## `/diff`

Draws the info panel in its files state, opening the panel if it is closed: a list of every file the
session wrote, with the lines added and removed, and every edit to the highlighted one under it. `q`,
Escape or Ctrl-C leave the state and put the panel back as it was. Typing `/diff` does not, since no
character reaches the box while the state is open; a second `/diff` queued earlier, while a compaction
or an aside ran, does leave it. Clicking the count in the Files section of
the panel draws it too. It needs a terminal at
least 100 columns wide, and says so on a narrower one. With no file written it says so and changes
nothing. It shows what the session's own write calls changed. A change another program made is not
in it. [Telling sessions apart](../using/sessions.md#telling-sessions-apart) has the keys.

## `/unfinished`

Lists what this session left unfinished, found by fixed rules over bravebot's own records and never by asking a model:

| What is open | Shown with |
|---|---|
| changes not built: a file was written after the last command ran | the files written since that command |
| the last command failed | the command and how it ended |
| open todos | each task not done and not cancelled |
| kept checkouts | each checkout's number and place, and the `/checkouts` line to apply or remove it |
| files not committed | each file this session wrote that differs from `HEAD` |
| a stopped goal | its condition, and whether it is paused or stopped sending the work back |
| running jobs | each job's name and how long it has run |

It starts no turn and sends nothing to the model.

**The line after a turn.** When a turn ends and the set of open loops has changed since the last turn, the session shows one line, such as `open: changes not built, the last command failed, 2 todos (planner); 1 kept checkout (you); 1 job running`. It gives counts only, grouped by whose move each is: the planner's, yours, or a wait. The same set is not repeated every turn, and `open: nothing` says the last one closed. The planner is told the same counts at the start of its next turn, and a resumed session works them out again from its record and the files on disk.

## `/todos`

Prints the session's plan in the transcript: every task, in the plan's order, with its status in
words beside its mark (`pending`, `in progress`, `completed` or `cancelled`) and its text wrapped
to the width and never cut. A heading above it says how many tasks are done, and how many are
cancelled where any are. The info panel draws only the rows it has room for, each cut to its width,
and this is the whole plan. A session with no plan says so.

The plan is the one the panel draws, and while a turn runs it is the list that turn has reported so
far. A task that is neither completed nor cancelled is drawn with the mark and ink the panel gives it
when you type the command: filled while a turn is running, and hollow and dim while none is, when the
whole row matches the panel's. What is printed is the
plan as it stood then, so a later report from the planner, or the end of the turn, does not change
it. It is no part of the conversation: no turn starts, nothing is sent, and it is not saved with the
session or put in an export. The text of a task is the planner's, so a control character in it is
drawn as a visible character, and every row of it has a margin of the interface's own that the text
cannot draw. The word takes no argument: `/todos` followed by words is a prompt. It touches nothing
a running turn holds, so typed while a turn runs it is carried out at once. A prompt file or skill
named `todos` is shadowed by the command.

Alt-A does the same and is one of the [chords you can move](../customize/configuration.md#keybindings), the `todos`
action. It works at the box, and while a turn runs. The info panel's plan ends with a row that names
the chord in force, where the plan has room for it.

## `/mcp [list | reconnect [alias] | disable <alias>]`

Bare or as `list`, it names each [MCP server](../customize/mcp-servers.md) the session runs and how its
tools stand, and each server your settings request that is not running, saying whether it was turned
off. `reconnect` starts every requested server that is not running and was not turned off here, and
`reconnect weather` starts that one, including one turned off. A start takes the road a session takes
as it opens, so a server whose declaration changed, or that nobody approved, is put to you again.
`disable weather` stops the server and drops its grant. It writes nothing, so the next session starts
the server as before. Typed while a turn runs, it waits for the turn to end. See
[starting or stopping one during a session](../customize/mcp-servers.md#starting-or-stopping-one-during-a-session).

## `/caffeinate`

Keeps the computer from going to sleep while a turn runs or a loop waits for its next tick,
including a wait the model asked for, and lets it sleep again once nothing is pending. Typed again,
it turns off. It is off when a session starts, and a watch waiting on a file does not count as
pending.

Only idle sleep is held off. The display can still turn off and the screen can still lock, but the
machine keeps running with your credentials on it while you are away, so use it only where your
device policy allows that. The first `/caffeinate` says this and turns nothing on; the second turns
it on and records your answer in `~/.bravebot/caffeinate`, so later sessions turn it on at once. An
incognito session asks every time.

It runs `/usr/bin/caffeinate` on macOS, `systemd-inhibit` on Linux and PowerShell on Windows. On Linux
`systemd-inhibit` is given the full path of `cat`, found only in directories a confined command cannot
write. If there is no such `cat`, or the program cannot be started, or stops by itself, the transcript
says so and `/caffeinate` turns off.

## `/thinking`

Shows the model's thinking as it arrives, and typed again hides it. It is off when a session opens
and applies to this session only. While it is on, the last few lines of what the model has thought
this round are drawn dimmed, under a heading, above the reply as it is written, and they go when the
round's words arrive. Only a service that sends its thinking shows any: Bedrock sends it when asked,
and Brave's endpoint or an OpenAI-compatible gateway shows what a model sends in `reasoning_content`.
The thinking is for the screen alone: it is not saved to the session, not exported, and not sent in a
later request, and turning it on changes nothing the model is asked. The desktop window has no
command: a Thinking button appears in the line that says a turn is running once the session has been
sent thinking, and shows the last eight lines of it under that line on the same terms.

## `/checkouts [apply <n> | remove <n>]`

Lists the checkouts this session keeps for its delegates, brings the files written in one back into your working directory, and removes one by its number.

```
/checkouts            # each kept checkout, numbered, with what was written in it
/checkouts apply 2    # bring back the files written in checkout c2, one question each
/checkouts apply 2 src/a.rs notes.md   # bring back only these two
/checkouts remove 2   # delete checkout c2, and its entry in your repository's .git
```

A delegate's [checkout](../customize/agents.md#a-checkout-of-its-own) is kept when it did something
there. The list names the paths the delegate typed for the files it wrote, twenty at most, and counts
the writes it made through a reference. Nothing reads a checkout's status, so a file a program
changed there is not named.

**`/checkouts apply 2` asks about every file the driver recorded a write to in checkout c2**, one at a
time, whatever your trust map would have said, and shows the difference between your file as it is
now and the checkout's. It is the same operation as the `apply_checkout` tool and asks the same
questions. A file a program wrote there, or wrote through a reference, is not among them.

**Paths after the number bring back only those files.** Each has to be a path the driver recorded
for that checkout, spelled as `/checkouts` lists it. If one is not, nothing is brought back. Paths
are split at spaces, so a path containing a space can only be brought back with the whole checkout.

**Removing one something was done in asks first**, since removing it deletes that work. `y` removes it; `n`, Esc and ctrl-c keep it. `2` and `c2` name
the same checkout. The rules copied for it go with it, except one marking a file there untrusted,
which stays so that history showing the file keeps its label. A checkout that holds your working
directory, or a directory added with `/add-dir`, is kept: `/cd` out of it first.

**The list is held in memory.** After `/clear` it starts empty and the earlier checkouts stay on disk,
and `--resume` brings none back.

## `/plan [task]`

Sets [plan mode](../security/permission-modes.md), as the mode key does when it
reaches planning. With a task it also starts a turn on that task, which runs in plan mode.

```
/plan fix the auth bug
```

The turn is sent the task and not the line, so it never sees `/plan`. Bare `/plan` sets the mode and
sends nothing. There is no command for bypassing every check. Typed while a turn runs it waits for
the turn to end, and then sets the mode.

## `/manifest <task>`

Plans one task in full, shows you the plan, then runs it with nothing re-planned.

```
/manifest add a --verbose flag, wire it through, and add a test
```

**It is a run, not a mode the session holds.** The session starts one run, waits for it, and comes
back to the turn loop. It is blocked for the duration: you can read, edit and stop, but not send.

The conversation is neither read nor written. The task string is what the planner gets, along with
any picture you pasted or dropped beside it, so nothing else from the conversation goes in, and a step's result is quarantined with no planner left to show it to,
so nothing comes back out. What the transcript shows is the goal as the planner understood it, the
frozen plan, each step as it runs, and the reply. The run leaves the conversation exactly as a
declined plan leaves the workspace.

**You approve the plan before the first step**, once, and the approval does not cover the writes. A
run you stopped is not written down, because there is nothing in it to read. The run is recorded as its
own record and the session records its name, so the session still resumes as a conversation.

In [plan mode](../security/permission-modes.md) a plan with a write in it does
not run at all, decided from the frozen plan before the plan is put to anybody. See
[Plan first, then execute](../using/manifest-mode.md) for the `--mode manifest` form.

## `/memory`

Lists the [memory](../customize/agents.md#memory) of each definition this session resolved that
keeps one, one line each: the definition's name, the path of its file, and where it stands.

```
/memory
```

The standing is what a run under the definition is told about the file. **Withheld** means your
[trust map](../security/trust.md) does not trust the path, whatever is or is not there, so a read of
it is quarantined. It says so where a write left the path untrusted and that is still recorded. **Not
read** means the path is a link, is reached through one, or is not a file. Otherwise the line says
nothing is kept there yet, or that a file is.

The command shows paths and standings and never the file's bytes, so it reads nothing from a memory
and has nothing to show of one that is withheld. Open the file yourself to read it. The word takes
no argument: `/memory` followed by words is a prompt.

It also lists the rule files you keep, each with its path and line count, and each fact in scope with
its key, whether it is for this project or every project, who asked for it, its label, the date it
was last verified, and whether another fact replaced it. A fact's text is not shown.

## `/init`

Has the planner draft an `AGENTS.md` for the project you are in, the file bravebot reads before every
turn to learn how work is done there. It sends a fixed request for a short guide titled "Repository
Guidelines" covering the project's structure, its build, test and development commands, its style,
its testing, and its commit and pull request conventions, and the planner writes it with
`write_file`, so you are shown the file and asked before it is written.

In a project you vouched for, the planner reads the project's files and drafts from them. In one you
did not, it cannot see them, so it asks you questions and writes from your answers. Either way, an
`AGENTS.md` written under that name is read from your next turn on.

If the directory already holds a file called `AGENTS.md`, `/init` says so and does nothing, whatever
the file contains. A `CLAUDE.md` does not count, only the name `AGENTS.md` is checked, and an `AGENTS.md` written beside a
`CLAUDE.md` is the one read, since the first file found is the only one.

## `/review [target] [focus]`

Has the planner review changes and report what it finds. The target is one of:

| Target | Reviews |
|---|---|
| none | the uncommitted changes, staged and unstaged |
| `staged` | only what is staged |
| `since <ref>` | everything since a branch, tag or commit, uncommitted changes included |
| `commit <ref>` | one commit |
| `pr <number or URL>` | a pull request, by number or by its `https://github.com/<owner>/<repo>/pull/<n>` address |

Words after the target are what to look at: `/review since main the locking in the cache`. Those
four words only count as the first word, so `/review the staged changes` reviews the uncommitted
changes with that focus. A ref that `git` could read as an option or a range, or an address that is
not a GitHub pull request, is refused with a usage note and no turn starts.

The planner fetches the diff with `run` (`git diff`, `git show` or `gh pr diff`), so you are asked
about the program like any other. Where the diff comes back as lines you have vouched for, the planner
reviews it. Where it comes back as a reference, the planner may not see it, and hands it to a
processor whose findings are shown to you as untrusted content and to no model. If the working
directory has a `REVIEW.md`, the planner reads it first as the project's notes on what a review
should flag or leave alone. It sees the file only where you have vouched for it, and reviews
without it otherwise. The command writes
nothing and the prompt forbids changes, commits, pushes and comments. A prompt file or skill named
`review` is shadowed by the command.

## `/commit-message [source] [format]`

Drafts a commit message from changes and puts it in the input box. The source is one of:

| Source | Drafts from |
|---|---|
| none, or `--staged` | what is staged, the index against `HEAD` |
| `--range <a>..<b>` | the changes between two commits, or `<a>...<b>` from their merge base |

Words after the source are the format you want: `/commit-message --staged use Conventional
Commits`. The two flags only count as the first word, so `/commit-message use Conventional Commits`
drafts from the staged changes in that format. A range that is not two plain revisions is refused
with a usage note.

The planner does not see the diff. The program reads it without starting `git`, under the rules
`read_git` is held to, and gives it to a processor that has no tools and no conversation. The
draft comes back into the box after the line `Use this as the commit message:`, so the line you
send always starts with those words, whatever the draft starts with. Edit it, then Enter sends it to
the planner as your own prompt, for instance to ask for a commit with that message. Escape drops it.
Nothing is committed by the command itself.

Nothing is drafted for a repository you have not trusted, for changes that show no file, or for a diff
that holds what looks like a credential, and the line says which. A diff longer than one read takes is
drafted from the part that was read, and the line says so. The model is the one `processorModel` names,
or the session's. Typed while a turn runs it waits for the turn to end.

## `/help <question>`

Asks the planner to answer a question about using bravebot from the documentation carried inside
the program, so the answer comes from the version you are running and not from a checkout or the
website: `/help how do I stop a background job`. The request names the built-in `bravebot-help`
skill, whose pages the planner loads one at a time, only the ones the question needs. The skill also
holds the clause headings of the specifications, for a question about exactly what bravebot does. If
the pages it loaded do not answer the question, it says so. Without the command, the planner loads
the same skill on its own when a question is about using bravebot.

The question is sent as you typed it, inside a sentence the program wrote, and the line you typed is
not sent. It is a line you typed, so a file you name in it with `@` is read as it is in any prompt.
A picture pasted or a file dropped into it is written as words, as for every command that does not
send them on.

With no question, `/help` says what it takes and nothing starts. A longer word such as `/helpful`,
and a sentence that mentions the word, is a prompt. Typed while a turn runs, the line waits for the
turn to end.

A skill named `bravebot-help` of your own, or one in a project you trusted, replaces the built-in and
its pages, and `/help` then loads that one. A prompt file or skill named `help` is shadowed by the
command.

## `/changelog`

Shows the release notes of the version you are running: a heading naming the version, then its
entries, each link written as its text. The notes are carried inside the program, so nothing is
fetched and no turn starts. Where the notes carry no heading for the version you run, the heading says
it is showing the newest release's, and which version runs. Between releases the top section holds
the notes of changes not yet released, headed with the last release's version, so a build made from
the commits after a release shows them under that version's number.

Only that one release is shown, and it is drawn in the transcript, so a release longer than the screen
shows its end and the heading is above it. The word takes no argument: `/changelog` followed by words is a
prompt. It touches nothing a running turn holds, so typed while a turn runs it is carried out at once.
A prompt file or skill named `changelog` is shadowed by the command.

## `/agent <name> <task>`

Runs one of your [delegate definitions](../customize/agents.md) on a task yourself, rather than
describing the work and hoping the planner picks it.

```
/agent rule-reviewer check the diff on this branch
```

**It is your turn, run under the definition.** The definition supplies three things: the
instruction in its body, its `model`, and its narrowing (its `kind` and its `tools` line). Nothing
else changes. The run can still ask you a question, keep a task list and ask before every write, and
what it reads and answers stays in the conversation, exactly as any turn of yours does. The line
after it goes to the session's own planner again. There is no mode to leave. Stopping the turn
before it did anything puts the whole `/agent` line back in the box, so Enter addresses the same
definition again. To address one definition on every line instead, start the session with
[`--agent <name>`](cli.md#--agent-name).

**It can only take away.** The turn holds what the session holds, cut down to the definition's kind
and its `tools` line. A `reader` addressed from a session that may write is a turn that may not. A
`worker` addressed from a session that may only read gets no write, and a tool the definition names
that the session is not offered is dropped and said in the trail. A tool the definition left out is
refused if the model calls it anyway. Five tools a delegate never gets (asking you, the task list,
fetching a URL, vetting content and spawning a delegate) are offered here, because each is withheld
from a delegate for a reason about nobody watching it. Scheduling a next turn and watching a file are
not: the turn either one starts is the session's planner's, holding everything the definition took
away. To have a definition look again, address it again.

**The name is compared against what this session resolved**, from `~/.bravebot/agents` and from a
project you vouched for. The bare word lists those names, and so does a name that matches nothing:

```
/agent
this session resolved reader, checker, worker, rule-reviewer; address one with /agent <name> <task>

/agent auditor check the diff
there is no definition called auditor; this session resolved reader, checker, worker, rule-reviewer
```

A definition in a project you did not vouch for is not in that list, and no spelling of its name
reaches it. The names are never offered as completions: you type the whole name every time, because
a completion row is one keystroke from being sent and a name is text somebody else may have written.

**A definition naming a model you have not signed in to does not run**, and says which definition
asked for which model, rather than running on the session's model instead:

```
rule-reviewer asked for haiku, which needs a sign-in first, so it did not run
```

A reply answered by a different model than the one named says that too. The reply is drawn under
the definition's name (`rule-reviewer answered`), taken from the name that matched and never from
anything the reply says about itself. The name is not written into the session record, so a
resumed session draws the same reply without it.

Only a line you typed into the box addresses a definition. `/agent` in a reply, in a file, or in a
line this program wrote is text.

## `/rename <name>`

Rewrites the session record immediately, and the chosen name survives the next turn. An empty name is
refused. Typed while a turn runs with nothing waiting, it is carried out at once rather than waiting
for the turn to end. Renaming leaves `/undo` nothing to go back to, including the turn running when
you renamed.

## `/note <note>`

Adds a note to the session record, on the turn the session is on: the running turn, or the last one
that finished. The record is rewritten at once, and the note is kept with the turn number and the
fact that you typed it. It is not part of the conversation: it is not a prompt, nothing waiting
moves, and nothing a turn sends to the model includes it. `/undo` is not affected. A resume, a
`/branch` and a continued session keep the notes. `/note` with nothing after it lists the notes the session holds, one line each with its turn. Where the session writes no record, as an incognito one does, it says the note reached none. Typed while a turn runs
with nothing waiting, it is carried out at once. `/session-review` reads the notes of a session it
reviews as leads from you. See [Sessions](../using/sessions.md#noting-something-on-a-session).

## `/issue` and `/pr`

`/issue <url>` and `/pr <url>` say which issue and which pull request the session is for, and the
[info panel](../using/sessions.md#telling-sessions-apart) shows both. Each takes one `http` or `https`
link with a host: a value with a space, a line break, an escape, a character outside ASCII or
another scheme sets nothing and is not repeated back. Alone, each says what is set, and `clear` removes that one. The session record is
rewritten at once, so a resume brings the links back, and nothing a turn reads includes them. Typed
while a turn runs with nothing waiting, each is carried out at once, as `/rename` is. [`--issue` and `--pr`](cli.md#--issue-url-and---pr-url) set the same links when a session opens.

## `/compact [focus]`

Summarises the conversation so far and keeps the recent part, on demand, at any size, without
consulting the budget. Anything after the word is taken as typed and tells the summariser what the
summary must keep: `/compact keep the lexer benchmarks and every path touched`. It is added to the
summariser's closing instruction and goes nowhere else, and the audit trail records that a focus was
given and its length, not the words. A `/compact` typed before any prompt writes no record, so it
does not give the session the title "untitled"; the first prompt that says something names it. The **request** is shortened, never the record: the replaced messages go to an
archive that the transcript still reads and the session record still stores. See
[Sessions](../using/sessions.md#long-conversations).

## `/recap`

Recaps where the session stands: what you are trying to do, how far the work has got, and what is
next, in at most 400 characters. It is a [`/btw`](#btw-question) with a fixed question: a copy of the
conversation goes out, no tools are offered, and neither the question nor the answer joins the
conversation. The answer is cut to 400 characters if the model writes more, and it opens in the same
mode as a `/btw` answer. `/recap` takes no argument: a line with words after it is a prompt.

A session also recaps itself once when you come back to it: the terminal has been unfocused for five
minutes (by default) since the last turn finished, and the session has had three turns. It does not recap again until
another turn finishes. The [`awaySummaryEnabled`](../customize/configuration.md#awaysummaryenabled)
setting turns that off, and [`awaySummaryIdleSeconds`](../customize/configuration.md#awaysummaryidleseconds)
changes the five minutes.

## `/btw <question>`

Asks something beside the work. A copy of the conversation goes out with your question on the end of
it, and neither half comes back into the conversation: the planner has read neither your question nor
the answer, and no later turn reads either.

The answer opens in the mode
[Ctrl-L](../using/interactive-mode.md#watching-a-delegate-reading-a-command-and-asking-something-aside)
opens, as a row of its own before the delegates, which is the one screen it exists
on. Typed while a turn runs, `/btw` is carried out as you press Enter: the question goes out beside the
turn over the exchange the turn last sent, the turn is not told, and the answer becomes a row without
opening the mode over the turn. It needs the turn to have sent a request, one question is out at a time
beside a turn, a dropped file is refused, and stopping the turn stops the question. Nothing about it is drawn among the turn's own lines, because an exchange drawn in the transcript
is one a reader takes the planner to have had.

One request, and no tools in it. **An aside cannot be asked about**: there is no box to follow up in,
so pressing further means a second `/btw`, over an exchange that still knows nothing of the first.

**The record keeps both halves**, so a resume brings the answer back into that view and into no
conversation. Where the exchange has already met something untrusted the answer is not written down:
the row says so while the words are still on screen to copy, and a resume brings back the question
alone. That is the same rule every message in the record passes, which is that nothing is written
that the planner could not have held.

## `/clear`

Begins a new session in this directory and keeps the current one resumable. Because it is a new
session it asks the trust question again, restores no standing permissions, and closes any directory
`/add-dir` had opened. The paths you let programs reach with [`request_path`](tools.md#request_path), the hosts you
allowed or declined, and your answers to the planner's questions end with it too, so each is asked
again. A running [loop](#loop-interval-prompt) or [goal](#goal-condition) ends with
the old session, and says so. With neither running it says nothing about them. The session's pull
request and issue are not carried over.

## `/branch [name]`

Copies this session as `bravebot --fork` does and moves you onto the copy, so you can try a second
approach without leaving. The copy has an id of its own and the conversation, spend history and audit
trail so far, and its title is marked `(fork)`, or is the name you give. The original is left exactly
as it was, and the transcript says its id and the directory it is in, so `bravebot --resume <id>`
returns to it.

A running [loop](#loop-interval-prompt), [goal](#goal-condition) or live watch is not written down
and ends, saying so. The copy starts with no turns for `/undo` to rewind. Nothing on disk is
rewound.

The copy holds what `--resume` of it would. The folders and programs you trusted stay trusted, and
you are asked again about language servers, run prompts already shown and files you agreed to show
despite the credential scan. Paths you let programs reach, hosts you answered and your answers to the
planner's questions are not carried either. The copy gets a scratch directory of its own.

It needs a record to copy, so it says there is nothing to branch until the first turn has ended. It
is refused in an incognito session, which writes none, and while the session keeps a checkout, which
a copy does not carry: remove it with `/checkouts remove` first. Typed while a turn runs it waits
for the turn to end.

## `/handoff <next goal>`

Starts a new session for the next piece of work, from a short brief instead of the whole
conversation. The planner writes the brief from this session, and it appears in the input box
followed by a sentence saying what did not come across, and the goal. Read it and change what you
like. Enter starts a new session whose first prompt is the box as it stands. Escape drops it.

Only the brief crosses. The files and folders you trusted, the programs you vouched for and the
other permissions you gave do not, so the new session asks again. The old session is left as it
was, and the new one's row in the `bravebot --resume` list says which session it came from.

It needs a record to link back to, so it says there is nothing to hand off until the first turn has
ended. A conversation that has met something untrusted offers no brief. Typed while a turn runs it waits for the turn to end. While a turn runs, Enter on a brief in
the box does not run its first line as a command: the brief stays in the box for the press after
the turn, and emptying the box ends the handoff.

## `/resume [id]`

Leaves this session for another one recorded in this directory, and carries on from it. With no id
it draws the list `bravebot --resume` draws, without the session you are in, and Enter picks the one
under the cursor. Escape, or Ctrl-C, returns to the session you were in. With an id it picks up that
record, so `/resume <id>` is `bravebot --resume <id>` without leaving the program.

The session picked up is restored as the flag restores it: its transcript, its spend, and the
folders, programs and rules you trusted in it, and only those. Nothing you granted in the session you
left comes with you, and its loop, goal and watches end. The session you left is written as its last
turn left it and stays resumable. Servers started for this process keep running.

It is refused, with a line saying why, for an id that is no session of this directory, for the
session already open, for a [manifest run](../using/manifest-mode.md#a-manifest-run-is-recorded-but-cannot-be-continued),
and for a session a running background session holds. Where this directory records no other session
it says so. Typed while a turn runs it waits for the turn to end. The word takes an id and nothing
else, and none of what you type after it is sent to the planner.

## `/bg`

Hands this session to a background process and leaves the screen. The process resumes the record
the session has written, opens idle in the same directory and in the permission mode this session
was in (asking, accepting edits or planning), and keeps running after the terminal closes. The
terminal prints the id to join it with: `bravebot attach <id>` draws the session as this screen
does, and `bravebot reply <id> "text"` sends it its next prompt. `bravebot sessions` lists it, with the session's title and its last prompt.

A background session holds a question it would ask until someone attaches, and a rule in a settings
file that allows something does not answer for you there. Stopping it, or leaving it idle for an
hour, ends the process and keeps the record, and the next `attach` or `reply` starts it again in the
mode every session opens in.

Inside an attached session, `/detach` ends the attachment and nothing else. See
[Sessions in the background](../using/sessions.md#sessions-in-the-background).

It is refused, with a line saying why and the session carrying on, in bypass, in an incognito
session, before the session has a record to hand over (send a prompt first), in a session started
with an option `--bg` also refuses (such as `--no-shell` or `--settings`), and while a loop, a goal
or a watch is running, which the background session does not run. It is also refused after `/sandbox`
moved the mode: a background session starts in the mode every session opens in, so the line says
`/bg` is refused because `/sandbox` moved this session, and to run `/sandbox` with the mode that
session starts in first. Typed while a turn runs it
waits for the turn to end. The word takes nothing after it: `/bg now` is sent as a prompt.

## `/export [path]`

Writes the transcript out as a markdown file under the working directory, at the path you name or at
`bravebot-export-<id>.md`. Without this the only way to get a conversation out is to read the session
record's JSON out of the state directory by hand.

The path is typed on the same line as the command, so it gets the confinement any other path from
that line would get: `..`, an absolute path and a drive prefix are refused, and containment is then
tested against the real location of the deepest directory that exists, so a path leading out of the
tree through a symlink is refused as well. Missing parent directories are created.

**Anything already at the path is refused rather than replaced**, a symlink whose target is missing
included. The file is written readable by you alone, as the record it came from is.

Typed while a turn runs, it writes the transcript as it stands and does not wait. A reply the turn is
still writing is not in the file, and neither is a prompt waiting behind the turn. The prompt the turn
is working on is marked `**Outcome:** still running when exported`. Without a path the file takes the
session's default name, which an export after the turn then finds taken, so give that one a path of
its own.

## `/copy [n | block [n] | text [n]]`

Puts the latest reply on the clipboard, and `/copy 2` the one before it. What is copied is the
markdown the model wrote, so it pastes as whole paragraphs, where a sweep with the mouse copies the
screen: the `⏺` before the first row, the indent before every other row, and a line break wherever
the terminal wrapped a paragraph. How many characters went is drawn at the right of the hint row
until your next prompt.

A reply is what the model said to you: the answer a turn or a `/manifest` run ends on and what it
said on its way to a tool call, a resumed session's included. Your prompts, notes, tool rows and the
files they showed, a delegate's work and a `/btw` answer are not counted. A control character other
than a line break or a tab is left out, as the screen leaves it out, so a pasted reply cannot carry
the sequence that ends a shell's bracketed paste. Its line breaks are kept, so a shell that does not
use bracketed paste runs each line as it arrives. A number past the oldest reply says how many there
are and copies nothing.

`/copy block` puts the last code block of the latest reply that has one on the clipboard, and
`/copy block 2` the one before it, counting the blocks of every reply from the latest. A code block is
the lines between two fences of backticks or tildes, and what is copied is those lines as the reply
wrote them, with no fence, the same as clicking `copy` at the right of the block's header. A
block with nothing in it, or nothing but blank lines, is not counted, as it has no `copy` to click.
It is counted when the row is too narrow to draw the word. Tab after `/copy b` finishes
the word. `/copy block` says so and copies nothing where no reply holds a block, where the number is
not one or is zero, and where it is past the oldest block, which it says how many there are.

`/copy text` puts the latest reply on the clipboard as the screen shows it, and `/copy text 2` the one
before it. It has none of the marks of the markdown the model wrote it in, such as `**`, backticks and
the `#` of a heading, none of the `⏺` and the indent the screen draws beside it, and no wrapping, so a
paragraph is one line. It is what the `copy as text` item of the right-click menu copies, and it is
refused as `/copy` is. Tab after `/copy t` finishes the word.

## `/open [edit] [n | address | path]`

Lists the web addresses and file paths in the latest reply that has any, one line each, numbered from 1
in the order they are drawn and as they are drawn. The reply still arriving is the latest where it has
one, and otherwise it is the newest reply that does. A link is found by the rules a click on it uses,
so one you can click is in the list, and a reply longer than the screen lists the links above and
below the view as well. A reply with more than 50 is listed up to the 50th, and a line says how many are
left out; a link past the 50th is opened by typing it after the command. It says so where no reply has
one.

`/open 2` opens the second link as a click on it does. An address opens in your browser, as a ctrl-click
on it does, and does so while a turn is running. A path shows the file at the end of the transcript, as
a click on it does, with the same refusals and the same notes, and shows nothing while a turn or a
command runs: it says that a file is shown when nothing is running, and you type it again then. A number
names the link in the list `/open` printed last, so a reply that arrives afterwards does not change
which link it opens. `/clear`, the start of another turn, a rewind with `/undo` or `/rewind`, and `/cd` end
the list, and a number typed before any list, after it ended or past its last link says so and opens
nothing.

`/open edit 2` does what a click with ctrl on the second link does: a path opens in your editor, at the
line and column it gave, started as the click starts it, and an address is refused, since the browser
opens it. The editor takes the terminal, so it is not started while a turn or a command runs, where the
command says so and you type it again. Tab after `/open e` finishes `edit`.

`/open https://example.test/guide` opens the address you type, and `/open src/main.rs:12` shows the file you
name, from the line after the colon, and `/open edit src/main.rs:12` opens it in your editor. Typing the
exact address or name is what asks for it, so it need not be in a reply. An address is checked as a click's
is: only a web address with a host, as `http` or `https`, in plain ASCII, and one that names a user or holds
anything else is refused and not opened. Any other text is a file name. It is resolved as a click on a path
resolves one, so a name outside the directory the session works in, or that is a directory, or that is
not there, is refused in the same words, and a name with no extension or with a blank in it, which a click
could not reach, works when the file is there. Digits alone are a number, so a file named by digits is typed
as `./12`. `edit` with nothing after it says what the command takes.

Typed while a turn runs, the list answers at once, and so does an address, as a click on one opens it
then. A file name is not queued: it says that a file is shown when nothing is running, and you type it
again. While a summary is being written, or an aside with no turn running, a line waits like every command, and a number is read
against the list it was typed beside.

## `/undo`

Puts the session back where it stood before the most recent turn, and **saying it again goes back
another**. Every path in the project a rewound turn wrote through a file tool goes back to what it
held first, and a file one created is removed; where two rewound turns wrote the same path, it goes
back to what it held before the first of them.

The conversation, turn count, spend, timing, remembered commands and transcript return to the
snapshot before the earliest rewound turn. Those turns' audit lines are dropped too. The prompt of the
earliest turn rewound comes back into the input box so you can edit it and send it again. It arrives as
a prompt (shell mode is switched off) and only into an empty box, so a line you have typed since is not
overwritten, and not while a turn is running.

File trust follows the bytes that remain. A restored file gets the lower of its trust when backed
up and its trust before the turn. A file left untouched gets the lower of its current trust and its
trust before the turn. Grants made during undone turns are withdrawn; distrust on an untouched
file stays in place until you grant trust again. A failed restoration leaves that path untrusted,
but does not withdraw unrelated grants.

Rewinding past the first turn removes the record only when restoration is complete, there are no
coverage warnings, and file trust matches the original snapshot. Otherwise the resulting state
stays saved. A name you gave the session before that turn stays with it.

**Five turns back is as far as it goes.** The turn that just ended is the one least likely to need
rewinding, because it is the one still on the screen; what people notice late is a mistake made two or
three prompts ago, after approving several diffs in a row. Depth stops at five because every point
holds a copy of the conversation as well as the bytes, and one is written after every turn whether or
not it is ever read.

**A rewind names any file it could not put back**, and the rest of the rewind still happens. What is
kept is bounded twice over: a session remembers its last five turns, and what those turns wrote over
is held to one budget between them rather than one each. Past it the turns furthest back are dropped
whole, and the most recent is kept whatever it cost. Inside a turn the same budget decides a path:
past it the path is still remembered but its contents are not, and a rewind reports it as a path that
would not go back rather than as a file that was never there.

**The points survive closing the program.** They are written into the session record with the
conversation, so `/undo` and `/rewind` after a `--resume` reach the same turns they reached before.

**These changes outside a turn give up every point at once**: `/clear`, `/compact`, `/btw`,
`/rename`, `/branch`, `/add-dir`, and `/cd`. Every point goes rather than the most recent alone, since such a
change lands after the most recent point and so before none of them. After that `/undo` says there is nothing left to undo
rather than rewinding to a point describing a different session.

**Running a command keeps undo available.** Commands, hooks, scratch writes, language servers, desktop turns and delegate checkouts can make
changes outside file-tool backups. Undo names the recorded causes in a
warning that some changes may remain. It still restores available backups, which can also overwrite
later command changes to those same paths. Failed restorations are reported separately by path.

## `/rewind [turns]`

Reads the rewind points before acting on one.

```
/rewind          # list the points, most recent first, numbered from one
/rewind 3        # go back three turns
```

Each row says how many turns back it is, which turn it would land before, what that turn was asked,
and **every path that turn wrote over**, or that it wrote over none. `/rewind <n>` then goes back that
many turns, which is what `/undo` said n times does, so what it puts back is every row from the first
down to the one chosen. As with `/undo`, the prompt of the earliest turn gone back past returns to the
input box as a prompt, when the box is empty and no turn is running.

A rewind acts the moment it is typed and it overwrites files, including edits you made yourself since
the turn. Deciding to run one is deciding about those files, so they have to be readable first, for
the same reason a write is shown as a diff before it is approved rather than reported after. The paths
are named rather than counted, because a count decides nothing.

`/rewind` given something that is not a number says what it takes. **A number past what the session
remembers rewinds nothing** and says how far back it does go, rather than going as far as it can:
somebody who asked for four turns and got two would be reading a tree two turns younger than they
believe it is.

It is a word of its own rather than an argument to `/undo` because a command that takes no argument is
only ever the bare word, which is what keeps `/undo the last thing I asked for` a prompt. A surface
that takes a number cannot also have that.

:::caution
**A rewind sees file-tool writes and nothing else.** A turn that changed a file by running a program
leaves nothing to put back: those changes stay on disk while the conversation says the turn never
happened. And what goes back is what a path held *before the turn wrote to it*, so an edit you made
yourself in between is lost. Nothing compares the file against what the turn left there, and nothing
asks first.
:::

## `/hunks [number]`

Lists the hunks the last turn wrote, and puts one of them back. A hunk is a run of changed lines
between what a file held before the turn and what it holds now.

```
/hunks           # list the hunks, numbered from one across every file the turn wrote
/hunks 2         # put hunk 2 back as it stood before the turn
```

Each row gives the number, the file, the line the hunk starts at and how many lines it added and
removed, and draws the hunk with a few unchanged lines around it. The list stops at 100 hunks and
says how many more there were, and `/undo` puts those back with the rest of the turn. When the turn
also changed files in ways a rewind does not account for, such as through a command, the list says
it may be missing changes.

`/hunks 2` writes the lines hunk 2 replaced back into the file and leaves every other line as it is
now, then lists what is left, because the numbers below the hunk have moved. `/undo` afterwards still
puts the whole turn back as it stood before it.

**A number names a hunk of the list you read.** It is refused, and nothing is written, when you have
not listed yet, when another turn has run since the list, or when the file no longer holds the hunk as
listed. That last case is how an edit of your own is protected: the file is compared with what the
turn kept, and a hunk that moved, grew, shrank or went is not the one you chose.

**Only files you vouched for are compared.** Comparing two texts is a decision, so a file the session
does not trust is named with the reason instead of being listed, and none of its lines is drawn.
`/undo` is the way back for it, and it puts back the rest of the turn too. A file the turn created, one
whose earlier contents were not kept, one that is not text, and one that holds what it held before the
turn are named the same way.

The file's trust is what it was before: it is withdrawn while the write runs and given back once it
has succeeded, and a write that fails part way leaves the file untrusted, says so, and is saved to the
session's record.

:::caution
**The planner is not told.** The conversation still describes the file as the turn left it, so a later
edit that names the lines the hunk held is refused for not finding them, and a later write of the whole
file from what the conversation holds puts the hunk back.
:::

## The rules every command shares

**Only a line a person typed into the box.** A command is dispatched from a key press and from nowhere
else: never a line the planner produced, never text read out of a file, never anything a processor
returned, never a line reconstructed from a transcript. A model that writes `/clear` has written four
characters, and they reach your screen as four characters.

Every command here decides something a turn is not allowed to decide on its own: which directories are
reachable, what the conversation consists of, which model thinks. The endorsement is the keystroke, so
the keystroke is the only thing that may produce one.

**The whole word, and an argument only after a space.** `/statusline` is not `/status`, and
`what does /add-dir do` is a question. The set of words this program claims is taken out of the
language you can use to talk to the planner, so it is claimed as narrowly as possible. The bare word
with nothing after it is the command with an empty argument, answered by saying what it needs, or by
doing what the bare word means where it means something of its own (`/loop` says what is repeating),
rather than by doing nothing quietly.

**In shell mode the line is a command line, not a command.** `! /usr/bin/env` runs a program. Nothing
is offered for completion there either, since `/usr/bin/env` is a path, and a turn running changes
none of that: the line waits behind its `!` and is run when the turn ends, rather than being answered
as a command or sent to the model. [Shell mode](../using/shell-mode.md) is the rest of it.

**A command is never sent as a prompt.** A line that is a command is acted on and does not reach the
model. A session asked to shorten itself must not answer by talking about shortening itself.

**The argument is taken verbatim**, spaces and all, with the surrounding whitespace trimmed and
nothing else done to it. A leading `~` is expanded only as a whole first segment, so a directory whose
own name begins with a tilde is not a home-relative path. Nothing shortens it, splits it, or asks the
planner what it meant. The one exception is the marker for a picture you pasted or a file you dropped
beside the line. A command that cannot carry it gets words in place of a picture and the file's name in
place of a drop. `/btw`, `/manifest`, `/plan` and `/loop` send their argument, so a marker stays in
it and the picture or file goes with it.

**While a turn runs the word waits, unless it touches nothing the turn holds.** `/cost`, `/context`, `/limit`, `/copy`,
`/watch`, `/outputs`, `/panel`, `/diff`, `/todos`, `/caffeinate`, `/thinking`, `/changelog`, `/open`, `/lsp`, and `/loop` and `/goal` in every form but the one that starts a loop or sets a goal, read or end only
what the session keeps for itself, so they are carried out as you type them, ahead of anything
waiting. `/jobs` is too: a stop only sets a flag the turn reads at its next step, as it reads the stop
key. So is `/export`, which adds a file under the working directory and replaces none. So is `/btw`, which is asked beside the turn over the exchange the turn last sent and changes nothing the turn holds, and which a line waiting ahead of it does not hold back; only a `/btw` already waiting does. `/exit` waits for the turn like the commands that end the session, and does not stop it. The exception is a line of the same command already waiting, which they wait behind, so
`/goal clear` typed after a waiting `/goal <condition>` clears that goal. `/theme` and `/config` change only how the screen
is drawn and what the box edits in, so they are carried out as you type them whatever is waiting. `/rename`, `/note`, `/issue`, `/pr`,
`/forget-trust`, `/model`, `/effort`, `/advisor`, `/style` and `/sandbox` change what the session keeps, and are carried out as you
type them unless a line waiting ahead of them could change what they act on. Behind such a line they wait too, so `/rename` typed after a
waiting `/clear` names the new session, and the line they wait behind is named under the turn and on their mark under the box (`after /clear`).
A `/sandbox` line waits only behind a line typed before it, and while it waits it changes the sandbox mode and nothing else, so it holds back none of them but another `/sandbox`: a `/note` typed
behind a waiting `/sandbox off` is written at once. `/issue` and `/pr` set a link on the session's record, which only `/clear`, `/handoff`,
`/branch`, `/resume`, `/bg` and `/exit` replace, so they are carried out as you type them behind any other waiting line, a prompt
included. `/rename` and `/note` act on the name and the notes the session's record holds. A prompt, a command that sends a request, `/clear`,
`/undo`, `/rewind`, `/hunks`, `/cd`, `/compact`, `/handoff`, `/branch`, `/resume`, `/bg` and `/exit` can change them, so they wait behind those. They are
carried out as you type them behind a command line, behind `/add-dir`, `/memory`, `/reach`, `/mcp`, `/style`, `/forget-trust`, `/issue`, `/pr`,
`/sandbox`, `/model`, `/effort` and `/advisor`, and behind a waiting `/note` or `/rename`: a `/note` typed behind a waiting `!make` is written at once.
`/model`, `/effort` and `/advisor` choose what a request is built with, so each waits behind a prompt, behind a command that sends a
request or leaves for another session, and behind the other two. Behind a command line, `/clear`, `/undo`, `/rewind`, `/hunks`, `/memory`,
`/reach`, `/branch`, `/add-dir`, `/cd`, `/mcp`, `/rename`, `/note`, `/style`, `/forget-trust`, `/issue`, `/pr` or `/sandbox` they are carried
out as you type them. `/style` picks what every turn opens its prompt with, so it waits behind a prompt, behind a command that begins a turn, behind `/clear`, `/cd`, `/add-dir`,
`/reach` and `/branch`, which move or end the paths a style file under `~/.bravebot` may not be read from, and behind `/handoff`, `/resume`, `/bg` and `/exit`, which
begin another session; behind a command line, `/compact`,
`/btw`, `/recap`, `/memory`, `/undo`, `/rewind`, `/hunks`, `/mcp`, `/after` and the settings commands above it is carried out as you type it. `/forget-trust` withdraws the answer kept about the
directory, which a program a prompt or a command line runs can change the meaning of by making a `.git` entry, so it waits behind those as well as behind the
commands that start a session here, move the directory or leave the session, and behind `/undo`, `/rewind`, `/hunks` and `/mcp`; it is carried out as you type it behind `/compact`, `/btw`, `/recap`, `/memory`, `/add-dir`, `/reach`, `/branch`, `/after` and the settings commands
above it. What they say goes into the history as you type them,
so it scrolls with the rest, and stays there when the turn ends. `/model`, `/theme`, `/effort` and `/config` alone open their list
over the turn, which keeps running behind it: Escape closes the list without stopping the turn, and
a question the turn asks takes the screen until you have answered it. A list still open when the turn
ends stays open. A model picked this way is the next turn's, and the turn running keeps its own.

Every other command typed mid-turn comes off the box and joins the lines waiting for the turn to end,
exactly as a prompt does: the box clears, the history remembers it, and it is drawn under the box
marked as waiting. It is never offered to the turn in flight, so nothing about it reaches the planner,
and when the queue reaches it, it is carried out rather than sent. The queue drains in the order you
typed, so a command behind a prompt waits for that prompt's turn. A waiting command says why it waits, in a line held under the turn. `/clear`, `/compact`, `/undo`, `/rewind`, `/cd`, `/add-dir` and `/exit` do not stop the turn, and Ctrl-Enter on one stops the turn and carries the command out. Nothing enters the transcript while
it waits, and taking back what is waiting gives the command back to the box like any other line.

During a compaction, a `/btw` question asked with no turn running, a `/manifest` run or a goal check, every command waits. A `/btw` typed during a turn waits for nothing and holds nothing back.

**A command name is written in this program, never read from a directory.** There is no way to add one
by putting a file somewhere.

## Skills after a slash

A skill's name completes after a slash: beneath the commands at the start of a line, and on its own
later in a sentence, as in `this is /release-no`. Taking one writes `/release-notes ` into the line,
and the line is still a prompt. The planner is told it is you asking for that skill and loads it, so
`/commit-style` never runs anything itself, and no skill becomes a command whatever it is called. A
word that is a command in full still lists the skills that extend it: `/pr` offers `pr-fix` beneath
the `/pr` command. See
[Skills](../customize/skills.md#naming-a-skill-after-a-slash).

## Prompts of an MCP server

A prompt a started [MCP server](../customize/mcp-servers.md#running-a-servers-prompts) offers is a
row after a slash, `/mcp__<alias>__<name>`, beside the skills and only at the start of a line. Taking
the row writes the names of the arguments that are not optional, as `name=`, and you type the values
and any optional ones as `name=value` pairs. Enter sends the line as a prompt, which fetches the
server's text. The text reaches the model as a reference and never as words, and a prompt is
never a command.

## Prompt files

A prompt you use often can be a file: `~/.bravebot/prompts/review.md` is run by typing `/review the
parser`. Enter replaces the line with the file's text and sends nothing, so you read it first and
press Enter again to send it.

```markdown
---
description: Review a change
agent: reviewer
---
Review $1 for races, with attention to $ARGUMENTS.
```

- `$ARGUMENTS` is everything after the name, and `$1` to `$9` are its words. A word the line lacks is
  empty.
- `agent` makes the box `/agent reviewer <text>`, which runs when you press Enter on it. `description`
  is not used yet.
- Only your own `~/.bravebot/prompts` is read, never a project's. A file cannot take a command's name,
  and a file whose text begins with a command word or would run an MCP prompt is refused, so no file
  carries out a command.
- A file that is unreadable, over 64 KiB or empty is refused with a note, and the line stays as typed.
- Names are not listed or completed.

## Not a command, but typed in the same place

| | |
|---|---|
| `@<path>` | include a workspace file as trusted context. [Adding context](../using/context.md) |
| `!<line>` | run a line in your own shell. [Shell mode](../using/shell-mode.md) |
