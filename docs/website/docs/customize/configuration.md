---
sidebar_position: 4
title: Configuration
description: What is baked into the binary, what lives in ~/.bravebot and beside your work, and the environment variables that override either.
---

# Configuration

There is one thing to set up before the first session, and it is which service answers a turn. A
released binary arrives pointed at Brave's own endpoint, which is not the same as having a service
configured to do this work: no account was named, no gateway was written down, and no subscription
was imported. So rather than open a session with nothing set up to answer it, a first run **says
what to configure and stops**. It names three routes, each by the thing to type or write:

| Route | Where |
|---|---|
| your own AWS account | [Reaching a model through AWS Bedrock](#reaching-a-model-through-aws-bedrock) |
| an OpenAI-compatible gateway, including a local one | [Reaching an OpenAI-compatible gateway](#reaching-an-openai-compatible-gateway) |
| a Leo Premium subscription you already have | [Leo Premium](premium.md) |

`bravebot doctor` says the same thing and fails while it holds. A run whose model a configured
service serves is not this case, whatever the Brave fields hold, and neither is a build pointed at a
host that is not Brave's: whoever pointed it there configured it. The check happens before a session
opens, so picking one of Brave's models from `/model` mid-session does not raise it again.

**If you have configured a service and still see this, you are one key from working.** A settings
block copied out of another tool names its models and names no default, so the model in force is
still the one the build came with. That case gets one line naming the [`model`](#model) key rather
than the three routes.

Once a service is configured, the rest of this page is what else you can set. What will actually be
used is reported by:

```sh
bravebot doctor
```

```
configuration OK
  offers    Brave Leo
  endpoint  https://ai-chat.bsg.brave.com/v1/chat/completions
  premium   https://ai-chat-premium.bsg.brave.com/v1/chat/completions
  key id    …
  model     automatic-bravebot (default)
  key       … (never transmitted)
  settings  no settings.json

confinement …
  mechanisms       …
```

`doctor` changes nothing. It reports a choice where one is in force rather than the default it
overrode, and a configuration error makes it fail rather than pass with a warning. The signing key is
named as never transmitted.

It reports **every backend this build can reach**, not just one. A machine with an AWS account
configured shows a second `offers` block with its region, profile and tiers (see
[Reaching a model through AWS Bedrock](#reaching-a-model-through-aws-bedrock)), and a configured
[gateway](#reaching-an-openai-compatible-gateway) shows a third, with its endpoint, its models, and
whether a credential was found for it. The `settings` line names which keys your settings file set,
and never their values. A file that sets no variables says so rather than being reported as an absent
file.

The `layer` lines name the [settings files](#settingsjson) in force, weakest first, and an `override`
line names a key more than one of them set beside the file that won it. Paths and names only: a value
from a settings file is never printed, because on some machines that value is a credential.

## `~/.bravebot`

Everything that should outlive a session lives here:

| Path | What it holds |
|---|---|
| `~/.bravebot/AGENTS.md` | standing instructions for every project |
| `~/.bravebot/skills/<name>/SKILL.md` | skills available in every project |
| `~/.bravebot/sessions/<directory>/` | session records and audit trails |
| `~/.bravebot/lsp/<workspace>/` | a language server's index, one per workspace ([`lsp`](../reference/tools.md#the-index-is-cached-and-it-is-not-small)) |
| `~/.bravebot/history` | prompts you have sent |
| `~/.bravebot/model` | the model chosen with `/model` |
| `~/.bravebot/effort` | the effort level chosen with `/effort` |
| `~/.bravebot/theme` | the theme chosen with `/theme` |
| `~/.bravebot/editor-mode` | the editing style chosen with `/config` |
| `~/.bravebot/themes/<name>.json` | themes you wrote yourself |
| `~/.bravebot/settings.json` | long-lived settings (see [below](#settingsjson)) |

An imported Leo Premium subscription is kept here too, in a file only you can read. See
[Leo Premium](premium.md#where-they-are-kept).

**Nobody else on the machine can read any of it.** On Unix, every directory bravebot makes under
`~/.bravebot` is reachable by you alone and every file it writes there is readable by you alone,
whichever part of the program is doing the writing. The history is the reason: it holds every prompt
you have typed, which means the paths you were working on, your branch names, and whatever you
pasted into one. A directory or file an older version left open to the machine is narrowed the next
time something writes to it, so an upgrade is enough.

Narrowing stops at `~/.bravebot`, and a symbolic link out of it is stepped over rather than
followed. What else is in your home directory is not this program's business, and somebody keeping
their sessions on another volume has put the target outside what bravebot was given.

A file **you** put there keeps the mode you gave it. `settings.json`, your standing instructions and
your skills are read rather than written, so nothing changes them; the directory's own mode is what
keeps them private.

Every operation here degrades to doing nothing. A missing home directory, a read-only disk or a
corrupt file does not stop a session starting.

`~/.bravebot` is one fixed name under the profile directory **the environment names**, and there is
no fallback. `HOME` names it on Unix. On Windows `HOME` is read first and `USERPROFILE` second, the
first of them holding a value answering, so a stock Windows install works and so does a Unix-like
shell on it. A value that is empty states nothing and is passed over.

Where none of them holds a value there is no directory at all: no settings of your own, no session to
resume, no history and no skills. Every part of the program does without in silence, so
`bravebot doctor` is where the absence is said out loud, naming which variables were looked at and
what is not kept without somewhere to keep it. A checkout's own settings, skills and `AGENTS.md` are
read regardless, those being beside your work rather than here.

Inventing a location is the one thing that does not happen. This is the directory whose contents are
trusted for being yours, so falling back to a working or temporary directory would put your prompt
history somewhere with none of that behind it.

:::note
Nothing recalled from `~/.bravebot` is fed straight to a turn. A recalled prompt is placed in the
input box, where you read it and press Enter. That keystroke is what makes it trusted, exactly as
typing it would have. A model name the server does not recognise is reset to the automatic entry
rather than obeyed.
:::

## Choosing a model

```
/model
```

opens a picker on the model currently in use, as a panel in the middle of the screen. The list comes
from the endpoint rather than from a set compiled in, so it is whatever the backend actually offers
today. The choice is written to `~/.bravebot/model`, so it outlives the session that made it and
applies in every directory. A one-shot run reads the same record, so a script uses the model you
picked unless [`--model`](../reference/cli.md#--model-name) names another.

**Type to narrow the list rather than arrowing through it.** A search matches the name shown, the name
a request would carry and the service that answers, ignoring case and anywhere in any of them, and
every word you type has to match something. The cursor stays on the model it was on while the list
narrows, and falls to the first match once that model no longer matches. A search matching nothing
says so, and there is nothing to select while it does.

Rows are **grouped under the service that answers them**, one heading per service, and the heading of
the section you are scrolling through is held on the top line.

`automatic-bravebot` lets the server triage per request, and is what an unrecognised name is reset
to. It is always offered, so it is the one choice that cannot fail to work. The model requested is not
necessarily the model used: some entries are weighted ensembles that resolve per request, and the
automatic entry itself picks per request.

**The roster is curated for Brave Bot.** Requests to Brave's endpoint carry a header naming this
product, and the endpoint answers with the models chosen for it rather than the full Leo list. Leo's
roster is chosen for a chat assistant and a good fraction of it cannot call tools at all, which would
make an agent that can read and write nothing. A gateway is sent no such header, since what a
third-party service gets is the shape it documents.

If you wrote `automatic` anywhere, it still works. That is Leo's triage entry and names a different
routing policy, so it is rewritten to `automatic-bravebot` before a request carries it, whether it
came from a settings file, an exported variable or a choice recorded by an older version. The
rewrite is one-way: `automatic` cannot be requested, and Leo's routing is not something the picker
offers.

The list is drawn for a person, and the names in it never reach a model. What you picked becomes the
`model` routing field of later requests.

With an AWS account or a gateway configured the picker offers those models alongside this list rather
than instead of it, each under its own heading. See
[Reaching a model through AWS Bedrock](#reaching-a-model-through-aws-bedrock) and
[Reaching an OpenAI-compatible gateway](#reaching-an-openai-compatible-gateway).

## Choosing how hard to think

```
/effort
```

opens a picker of five levels, cheapest first (`low`, `medium`, `high`, `xhigh` and `max`), above a
row for asking for no level at all. `/effort high` takes one without opening the panel. The choice is
written to `~/.bravebot/effort`, so it outlives the session and applies in every directory, and
`/status` reports it beside the model.

**Nothing infers a level.** Until you choose one the request carries no such field at all and each
service applies its own default. Taking the row for no level removes the record rather than writing an
empty one, which puts you back where you were before you ever chose. A word bravebot does not define
changes nothing and says so.

**A level goes only where the roster says it is read.** Reasoning is two parameters rather than one on
a gateway, so a model can reason and still not read a level sent this way. Where the listing describing
your model says it reads none, no level is sent and `/status` says so. The choice is kept either way
and applies again the moment you pick a model that reads one. A model no listing described is not a
model stated to read nothing: a name from your settings file, a roster reporting no parameters, and a
listing that could not be fetched all leave the level to go out and be judged at the far end. A row
that does advertise the parameter says it reads one without saying which words it accepts, so a model
may reject or silently round a level it does not know.

**Where nothing describes the model, the answer arrives as a refusal.** Neither AWS Bedrock nor a
settings block that names its own models says which parameters a model takes, so a level goes out to
be judged. A model that refuses the field is asked again without it and is sent no level for the rest
of the session, and the interface reports it as reading none rather than going on showing your level
as in force. What one model refuses says nothing about another, on Bedrock or on the same gateway. A
request refused with the level already gone settles nothing and is not remembered.

A level you wrote into a model's own [`options`](#what-a-models-options-can-and-cannot-do) is not
given up this way. That field is carried into the request as it stands, so it fills the level again
after the one you chose in the interface has been withdrawn, and a service that does not take it
refuses the request as it would refuse any other option it does not know.

:::caution
**The Brave endpoint accepts the level and discards it.** A nonsense value is answered exactly as a
real one is, and a model that reports a reasoning-token count reports the same count whatever level
was asked for. A level chosen against a Brave-served model is carried, sent and dropped, while the
interface goes on reporting it as in force. Bedrock and gateways are unaffected.
:::

## Choosing a theme

```
/theme
```

opens a picker that live-previews over your own transcript; `/theme <name>` applies one directly. The
choice is written to `~/.bravebot/theme`, so it outlives the session that made it and applies in every
directory, exactly as the model choice does. A name that matches no theme, and an empty or corrupt
file, is no choice at all and falls back to `brave`. A choice saved under the earlier name `system`
still finds `brave` rather than being silently lost.

A theme of your own is a JSON file under `~/.bravebot/themes/`, named for the theme: `nord.json` is
the theme `nord`. `brave.json` and `system.json` are refused, both names reaching the built-in theme.
Each key is one role, and any you leave out inherits from `brave`:

```json
{
  "defs": { "ink": "#cdd6f4", "shell": "#1e1e2e" },
  "background": "shell",
  "text": "ink",
  "muted": "#6c7086",
  "ok": "#a6e3a1",
  "fail": "#f38ba8",
  "running": "#f9e2af",
  "accent": "#cba6f7",
  "note": "#fab387",
  "primary": "#89b4fa"
}
```

A value is a `#rrggbb` colour, a name from `defs`, or `none` to leave that role to your terminal's own
default. A `defs` entry that names another `defs` entry is refused rather than chased, so a palette
cannot loop. A file that will not parse is left out of the list rather than stopping the session.

An ink may also be a **pair**, one colour for each terminal background:

```json
{ "muted": { "dark": "#6c7086", "light": "#8c8fa1" } }
```

The arm matching the background sensed at startup is the one used. A pair composes with `defs` and
with `none` exactly as a lone value does. A pair missing an arm is refused and the file holding it is
not a theme, because filling the missing arm in would let a typo paint half a palette. Write a pair
where a scheme was published for a light terminal and a dark one: that is one theme, not two. A theme
that gives at least one pair says so under the picker's list.

:::note
Themes are read from `~/.bravebot/themes` and from nowhere else. A `.bravebot/themes` directory inside
a project is **not** consulted. A repository you have just cloned must not be able to decide how your
interface is painted.
:::

See [Reading the transcript](../using/transcript.md#themes) for what each role paints.

## Choosing a language

Brave Bot reads the interface in your language where a translation for it has shipped, and in English
otherwise. It takes the first of `BRAVEBOT_LOCALE`, `LC_ALL`, `LC_MESSAGES` and `LANG` that is set, so
on a machine already set up for French there is nothing to do.

```sh
bravebot                          # whatever your shell says
BRAVEBOT_LOCALE=fr bravebot       # this once
export BRAVEBOT_LOCALE=fr         # from now on
```

`BRAVEBOT_LOCALE` puts this one program in a language the rest of your shell is not.

A request widens rather than failing: `fr-CA` and `fr-BE` are answered by the French catalog where
they have none of their own, and a language nothing has shipped for reads in English. `LC_ALL=C` asks
for no translation at all. English and French are what ship today. What a shell appends to say which
encoding or modifier it wants is not part of the name.

Widening is per message, not per language, so a single message a translation has not reached yet reads
in English inside an otherwise translated screen. A message that **counts** something is pluralised by
the rules of the language it was written in, which for one falling back is English's, rather than
having English's rule applied to French text.

### What stays in English

- **The names of the slash commands**, so `/model` is `/model` everywhere.
- **The letters a question is answered with**, `y` and `n`. These are both the key drawn and the key
  matched, so a French reader is told to press `y` for *oui*.
- **The audit trail.** It is fixed columns of gate and capability names that are identifiers, read
  against the specs that use those same names.
- **The words on the working indicator**, unless a language supplies its own list.

Digit grouping and currency forms are not localized either. A catalog says only what separates a
whole number from its fraction.

**Nothing the model is sent changes with your language.** Tool descriptions, the preamble and the
sentence a refused tool answers with all stay as they are, because the words in them are load-bearing
on what the planner does. Switching language changes what you read and never what the agent does.

## Environment variables

The environment wins when set, over both the built-in values and
[`settings.json`](#settingsjson). That is how a released binary is pointed at a local backend
without rebuilding it.

| Variable | What it sets |
|---|---|
| `BRAVE_AI_CHAT_ENDPOINT` | the host requests go to |
| `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` | the premium host, used once a subscription is imported |
| `SERVICES_KEY_AICHAT` | the services key requests are signed with |
| `BRAVE_SERVICES_KEY_ID` | the key id that goes with it |
| `BRAVE_AI_CHAT_DEFAULT_MODEL` | the model to request when nobody has chosen one |
| `BRAVEBOT_CONTEXT_BUDGET` | the token budget before a conversation is compacted |
| `BRAVEBOT_OUTPUT_BUDGET` | how far one reply may run before the service cuts it off ([below](#how-long-a-reply-may-run)) |
| `BRAVEBOT_LOCALE` | the language the interface is read in |
| `BRAVEBOT_SUBPROCESS_ENV_SCRUB` | `0` hands a program bravebot starts its own credentials ([`run.scrubEnv`](#runscrubenv)) |

Six more name an AWS account rather than this build. See
[Reaching a model through AWS Bedrock](#reaching-a-model-through-aws-bedrock).

To point a release build at a backend running locally:

```sh
BRAVE_AI_CHAT_ENDPOINT=http://127.0.0.1:8000 bravebot doctor
```

`BRAVE_AI_CHAT_DEFAULT_MODEL` is a **default rather than the setting**: `/model` picks one per user
and that choice wins, so this applies until somebody makes one.

`BRAVEBOT_CONTEXT_BUDGET` is never baked into a binary. It is a knob one person turns while working,
so it has to be set in the environment.

**Thirteen names can also go in a settings file's [`env`](#settingsjson) block**, under the same
spelling: seven of the nine above, plus the six AWS ones. The two exceptions are `BRAVEBOT_LOCALE` and
`BRAVEBOT_SUBPROCESS_ENV_SCRUB`, which are read from the environment alone. Exporting a name wins over
the file, except where an [administrator pinned it](#pinned-by-an-administrator).

## `settings.json`

Long-lived configuration can go in a file instead of your shell profile:

```json
{
  "model": "sonnet",
  "env": {
    "BRAVEBOT_USE_BEDROCK": "1",
    "AWS_REGION": "us-west-2",
    "AWS_PROFILE": "my-profile",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "arn:aws:bedrock:…"
  }
}
```

**Three files are found, the closest to your work last:**

| File | What it is for |
|---|---|
| `~/.bravebot/settings.json` | you, in every directory |
| `.bravebot/settings.json` | the directory you started bravebot in |
| `.bravebot/settings.local.json` | that directory, on this machine only |

[`--settings <path>`](../reference/cli.md) reads a **fourth** above those three, for one run.
A file [an administrator pinned](#pinned-by-an-administrator) answers above all of them, and above the
environment too.

A later file overrides an earlier one **a name at a time** rather than wholesale, so a file that sets
one thing leaves everything else in force:

| What | How the files combine |
|---|---|
| `env`, `provider`, `attribution`, `keybindings` | per name one level down; the value under a name is replaced whole |
| `run.scrubEnv`, `permissions.deny`, `permissions.ask`, `permissions.additionalDirectories` | every file's entries are kept |
| `permissions.allow` | your own file's entries, a `--settings` file outside the project, and a project's entries you granted |
| `model`, anything else | the closest file that set it wins |

The lists are the exception because an entry in one only ever takes something away: a name under
`scrubEnv` withholds a variable from a program, and a `deny` or `ask` rule refuses or asks about
something that was otherwise allowed. Overriding them would let a file closer to your work hand back
what a broader one withheld, and a permission removed by a file you never opened is the outcome
worth ruling out.

`allow` goes the other way, which is why it is the one list a checkout cannot write by itself. An
entry there stops a prompt appearing, so one in `.bravebot/settings.json` or
`.bravebot/settings.local.json` would let whoever last edited the repository approve a command, a
write or a fetch on your behalf. Entries in those two files are instead **proposed**: a session opening
in the project lists them in one box with the file each came from, and only your yes puts them in
force. The answer is kept per project in `~/.bravebot/granted/`, so you are asked once rather than at
every launch, and a rule the project edits afterwards is asked about again. `bravebot doctor` names
every such entry and says whether it is granted. A file `--settings` names is yours, since you typed
the path, unless it resolves inside the project you are working in, which makes it the checkout's file
under another name.

**The project files are read from the directory you started bravebot in, and from no directory above
it.** Searching upward would make what configures a session depend on which directory you happened to
change into, and the file it found could sit above the thing you are working on.

These keys are read, and anything else in the file is ignored rather than refused:

| Key | What it holds |
|---|---|
| `model` | the model to request when nobody has chosen one ([below](#model)) |
| `editorMode` | whether the input box edits the ordinary way or vi's ([below](#editormode)) |
| `env` | variables, in Claude Code's own shape |
| `permissions` | which actions to refuse, and which to ask about ([below](#permissions)) |
| `provider` | an OpenAI-compatible gateway ([below](#reaching-an-openai-compatible-gateway)), or an AWS account ([below](#naming-more-than-three-models)) |
| `run.scrubEnv` | further variables to keep from a program the agent runs ([below](#runscrubenv)) |
| `attribution` | what a commit message or a pull request this agent writes may carry ([below](#attribution)) |
| `keybindings` | keys rebound to your own choice ([below](#keybindings)) |
| `search` | how large a tree a search may walk ([below](#search)) |
| `vetting` | whether quarantined content is checked without asking you ([below](#vetting)) |

In `env`, only string values: a number or a boolean is skipped rather than coerced, so write `"1"` and
`"true"`. Every name in the block is read rather than a chosen subset.

**The file is the same shape as Claude Code's `~/.claude/settings.json`**, so a block that configures
one largely configures the other unedited. The three files resolve in the same order Claude Code's do,
down to the name `settings.local.json`, so knowing where to put a value for one tool is knowing it for
the other. Where a variable names **your** deployment the spelling is kept, which is why the Bedrock
tiers below are `ANTHROPIC_DEFAULT_*_MODEL`. The switch that decides which backend bravebot itself
uses is `BRAVEBOT_USE_BEDROCK`.

:::caution
`BRAVEBOT_USE_BEDROCK` was called `CLAUDE_CODE_USE_BEDROCK`. The old name now **reads as unset**, so a
file or a profile still setting it falls back to the Brave backend without an error. Rename it.
:::

**The environment wins over all three files.** A variable exported in your shell overrides the same
name here.

Two limits fail silently by design. A file over 64 KB is refused rather than parsed, and **every
failure is treated as absence** (no file, a syntax error, an unparseable value), because the built-in
configuration still describes a working backend. Nothing refuses to start over this. `bravebot doctor`
is where a file nobody can parse shows up.

Each of the three files fails on its own. One that is missing, oversized or unparseable leaves the
others in force, so a mistake in a checkout cannot decide that your own file no longer applies.

:::caution
**A `.bravebot/settings.json` arrives with a checkout.** A repository you have just cloned can name
the host every request goes to and the credential profile that signs it, and nothing on the screen
says so. Your conversation reaching a host the repository chose is the cost to weigh: read a
project's settings file before working in it, and `bravebot doctor` names the files in force. What
it cannot do is grant a capability. The names that would (`permissions.allow`, and
`permissions.additionalDirectories`) do not take effect on being read: each is a question you answer
when the session opens, the rules in one box listing them and the directories one box apiece.
:::

:::note
**What this file names is destinations, not capabilities.** A region, a credential profile, a model:
nothing in `env` vouches for a path, decides whether an effect is allowed, or names a command to run.
The file is the easiest thing on the machine to write to, so a capability grantable from here would be
a capability granted by whatever last edited it. It does not become the process environment either. A
value is consulted where a variable would be, and reaches a subprocess only where that subprocess is
the thing it configures.

A [`permissions`](#permissions) block can refuse an action and it can answer a prompt, and it can do
nothing else: no rule there makes a path reachable, and no rule makes a command's output trusted. Of
the two things it can do, only refusing is a claim a checkout may make on its own. `permissions.allow`
answers a prompt, so a project's entries are rules you are shown and grant rather than rules its file
puts in force, and `additionalDirectories` names directories you are asked about one at a time rather
than ones a file opens.

[`vetting`](#vetting) decides whether you are asked something at all, which is why it too is read
from your home file alone and never from a checkout's.
:::

### `model`

```json
{ "model": "sonnet" }
```

The model to request when nobody has chosen one. This is the one key in the file that **outranks the
model baked into the binary**. An exported `BRAVE_AI_CHAT_DEFAULT_MODEL` still wins over it, a choice
recorded by `/model` wins over both, and [`--model`](../reference/cli.md#--model-name) on a one-shot
run wins over everything.

`opus`, `sonnet` and `haiku` name a **tier** rather than a model, since that is what a settings file
written for another tool puts here. Each resolves to something reachable: the model your AWS account
named for that tier, and otherwise that tier's name on the Brave roster. A tier word is never sent as
written, because a service has never heard of it. Any other name is used exactly as you wrote it.
Bedrock refuses a model it does not recognise, and the aichat endpoint silently resets one to
`automatic-bravebot`, which is the key appearing to work while changing nothing.

### `editorMode`

```json
{ "editorMode": "vim" }
```

`vim` gives the input box [vi's editing keys](../using/interactive-mode.md#editing-the-way-vi-does)
and `emacs` gives the ordinary box. The word is read whatever its case. One naming neither style is
no choice at all: the box stays the ordinary one and nothing fails to start.

A choice made with [`/config`](../reference/commands.md#config) outranks this file, which answers for
somebody who has never made one. The style is a preference about the person rather than a property of
a checkout, which is why a file in a repository is the weaker claim.

### `run.scrubEnv`

```json
{ "run": { "scrubEnv": ["MY_TOKEN", "OTHER_SECRET"] } }
```

Variables to withhold from a program the agent runs, on top of bravebot's own credentials, which are
withheld with no configuration at all. See [`run`](../reference/tools.md#what-a-program-is-handed).

A hook and a language server are withheld these names too. A line you typed yourself at the `!`
prompt is not: it keeps your whole environment, because it is meant to behave as your own terminal
does.

**Your list is not read for a program bravebot starts for itself.** The `aws` CLI the Bedrock
backend runs to resolve a credential is withheld bravebot's own credentials and nothing else, since
a name here answers what the commands *you* ask for may see, and `AWS_PROFILE` in it would stop that
CLI resolving the credential it is being run for.

**Names only.** A list of names can only ever take something away; a list of values here would put a
credential in front of every command the agent starts. The list is read when the process starts, so
editing it describes your next session.

`BRAVEBOT_SUBPROCESS_ENV_SCRUB=0` turns the withholding off entirely. Only that exact spelling does
it: `false`, `no` and `off` change nothing, because a credential reaching every subprocess is not a
thing to switch off by near-miss.

### `permissions`

```json
{
  "permissions": {
    "deny": ["Read(.env)", "Edit(src/**)", "Bash(curl *)"],
    "ask": ["Bash(git push *)"],
    "allow": ["Bash(cargo test)", "Bash(ls *)", "WebFetch(domain:docs.rs)"],
    "additionalDirectories": ["../shared-lib"]
  }
}
```

The same three lists Claude Code keeps, with the same spellings, so a block copied out of
`~/.claude/settings.json` works unedited. What a rule is allowed to decide, and the reason it may
never trust a command's output, is on
[Approvals and permissions](../security/permissions.md#rules-you-write-down-in-advance).

`deny` and `ask` work from any of the files. `allow` works from `~/.bravebot/settings.json` and from
a `--settings` file outside your project; a project's own entries are proposed to you in a box when
the session opens, and work once you grant them. See [how the files combine](#settingsjson) for why.

A rule is `Tool` or `Tool(specifier)`, and names one of four **families**:

| Family | Covers |
|---|---|
| `Read` | every tool that reads or enumerates a file |
| `Edit` | every tool that changes one |
| `Bash` | running a program |
| `WebFetch` | fetching a URL |

These are categories rather than tool names, as they are in Claude Code, so there is no rule spelled
`Write` or `Glob`. `Bash` names no shell (there is none), and its specifier is matched against one
step's program and arguments.

**`WebFetch` takes `domain:` and nothing else.** `WebFetch(domain:example.com)` covers that host and
its subdomains, and never `notexample.com`: the boundary is a label boundary. There is no URL-prefix
form, since a rule matching a path would be answering a different question on every call. What a
matching rule decides for a fetch, and what it does not, is
[`fetch_url`](../reference/tools.md#fetch_url).

**`deny`, then `ask`, then `allow`, and the first match decides.** Specificity does not enter into
it: a broad deny beats a narrow allow, and a matching `ask` rule prompts even where a more specific
`allow` also matches.

**`allow` decides nothing for a one-shot run.** `deny` and `ask` hold there as they do in a session,
because both still decide something with nobody watching. See
[Non-interactive use](../using/headless.md#nothing-is-approved).

A **path** specifier is gitignore-shaped. `*` matches within one segment and `**` across them, and a
trailing `/**` covers the directory it names as well as what is under it. Four anchors decide where a
pattern begins:

| Written | Starts at |
|---|---|
| `//x` | the filesystem root |
| `~/x` | your home directory |
| `/x` | the directory the settings file is in |
| `x` or `./x` | the workspace |

So a single leading slash is **not** the filesystem root. A specifier with no slash in it is a name
and matches at any depth, which makes `Read(.env)` and `Read(**/.env)` one rule. Relative and
absolute patterns are separate namespaces and neither reaches into the other.

**A one-segment relative pattern floats where it restricts and not where it grants.** `Edit(src/**)`
in `deny` or `ask` covers a `src` directory at any depth, including a copy under `vendor`; the same
pattern in `allow` covers only the `src` at the top. Anchor it as `Edit(/src/**)` to pin it to one
place in either list.

A **command** specifier matches the whole line, with `*` standing in for any text:

| Rule | Matches | Does not match |
|---|---|---|
| `Bash(cargo test)` | `cargo test` | `cargo test --release` |
| `Bash(ls *)` | `ls`, `ls -la` | `lsof` |
| `Bash(ls*)` | `ls`, `lsof` | |
| `Bash(* --help *)` | `npm run --help x` | `npm --help` |

A trailing ` *` also matches the bare command, but only when it is the rule's only wildcard. The
space before it is part of the rule. A trailing `:*` is the same rule as a trailing ` *`, and a colon
anywhere else is an ordinary character.

**Every step of a command line is judged on its own**, as its program and arguments joined by single
spaces, which is the shape a rule is written in. Restricting any one step restricts the whole line.
Granting the line needs every step granted. An argument is never re-split, so a denied program cannot
be smuggled inside one.

A `Read` or `Write` rule reaches a line too: a redirection like `> notes.txt` is matched as a path,
exactly as `write_file`'s destination is, and a `<` is matched as a read. A rule about a path holds
whichever tool got there.

**`additionalDirectories` asks before it opens.** Each name is put to you as its own question when the
session opens, and one you accept is opened by the route [`/add-dir`](../reference/commands.md) takes
and trusted for the session on the same terms. One you decline is neither reachable nor trusted. A
relative name means a path under the workspace. See
[Trusted directories](../security/trust.md#every-way-a-rule-gets-written).

`defaultMode` is parsed so that a file carrying it is not rejected, and **acted on by nothing**: if
you wrote `acceptEdits` you get the prompts you would have got without it. The modes it names do
exist. Shift-Tab and `--dangerously-skip-permissions` are what choose one. See
[modes](../security/permissions.md#answering-in-advance-modes).

**An unreadable rule is dropped, named, and takes nothing with it.** A line that is not a rule, names
no family, or has an anchor that cannot be resolved is reported by `doctor` and in the session where
the file was read, and the rest of the file still applies.

The rules are read **once per session**, so a file you edit while a session is open describes the next
one. A session with no `permissions` block behaves exactly as one did before the block existed: every
gate asks what it asked before, and nothing is refused for being unmentioned.

### `attribution`

```json
{ "attribution": { "commit": "", "pr": "Co-authored-by: …" } }
```

What a commit message or a pull request this agent writes may carry. **The empty string is an answer
and means carry nothing**, which is the point of the block: asking for none of it in your standing
instructions puts the answer somewhere the model has to still be reading at the moment it writes one,
while a key states it once.

A name no file wrote is unset, which is a different answer from empty: it leaves the decision to
whoever writes the commit. Anything that is not a string reads as absence.

What you set here is stated to the model in front of every round of every turn, and a destination you
did not name is not mentioned to it at all. A value is quoted as text to copy rather than written in
as a sentence addressed to it, since the block resolves through a file in the checkout as well as your
own.

### `keybindings`

```json
{ "keybindings": { "stash": "alt-s", "scroller": "alt-o" } }
```

Seven actions can be moved and nothing else can. A chord is spelled `ctrl-x`, `alt-o` or `ctrl+x`:

| Action | Default | What it does |
|---|---|---|
| `editor` | `ctrl-g` | open the current prompt in your editor |
| `watch` | `ctrl-l` | watch a background delegate, or inspect what is running |
| `scroller` | `ctrl-o` | open the [transcript scroller](../using/transcript.md) |
| `history` | `ctrl-r` | search your prompt history |
| `stash` | `ctrl-s` | put the current line aside, or bring it back |
| `trail` | `ctrl-t` | toggle the [audit trail](../security/audit-trail.md) |
| `paste` | `ctrl-v` | paste from the clipboard |

**A chord has to carry Ctrl or Alt.** Every unmodified key is already answered (a character is typed,
Enter sends, Escape clears, Tab takes what is offered, the arrows move), so handing one to an action
would take it out of the alphabet. `shift-a` and `ctrl-shift-a` name an event a terminal never
reports, and Ctrl-C, Ctrl-D, Ctrl-J and Shift-Enter are refused because leaving and starting a line
are not rebindable.

**Every action keeps a key of its own.** A chord that cannot be read, or that the input box already
answers, leaves that action on its default. So does one two actions both asked for: both fall back
rather than one winning, since which won would come down to the order the file was read in. Two
actions *trading* chords is not a conflict and both get what they asked for.

The block layers per action the way `env` does, so a project file moving one action says nothing about
the other six. See [Interactive mode](../using/interactive-mode.md) for what the keys do.

### `search`

```json
{ "search": { "maxFiles": 500000, "maxSeconds": 60 } }
```

How many files a search may walk and how long it may spend opening them. Either may be **raised** as
well as lowered. The built-in caps are past what a repository people usually work in holds; a monorepo,
a tree of generated sources, or a checkout on a network filesystem is where they are not, and there
every search comes back partial. A partial search is the answer that reads like a complete one, which
is why this is worth setting.

The two are independent, so naming one says nothing about the other in any layer. A cap of zero, or
any value that is not a whole count, is absence and leaves the built-in cap in force rather than
permitting a search that reads nothing. Raising a cap does not unbound a search: the walk still stops
at `maxFiles`, the reading still stops at `maxSeconds`, and the match cap holds regardless of both. See
[`search`](../reference/tools.md#search).

### `vetting`

```json
{ "vetting": { "auto": true } }
```

Whether content nobody vouched for may be checked without asking you first. It is **off until you turn
it on**, and it is a boolean: `"true"` as a string, a number, or anything else is absence, so a file
that meant to turn this on and mistyped the value leaves the asking in place.

:::note
**This is the one block read from `~/.bravebot/settings.json` alone.** A `.bravebot/settings.json` in a
checkout that names it is reported by `doctor` rather than obeyed. Every other key here configures
where a request goes or how the interface behaves; this one decides whether you are asked before
content nobody vouched for reaches the planner, so a line in a repository you just cloned could
otherwise turn the asking off for whoever opened it.
:::

A choice you record for yourself while working outranks this file, and a flag outranks both.

## Pinned by an administrator

One file answers **above the process environment**, and therefore above every other source:

| Platform | Path |
|---|---|
| Linux | `/etc/bravebot/managed.json` |
| macOS | `/Library/Application Support/bravebot/managed.json` |
| Windows | `C:\ProgramData\bravebot\managed.json` |

The path is a literal, and no variable names it. `%ProgramData%` and the rest are stated in the
environment of the person this layer binds, so reading one would let them choose which file answers for
them.

This inverts the rule that the environment wins, deliberately. That rule exists so a released binary
can be pointed at a local backend without rebuilding it, and a pin an exported variable outranked
would pin nothing. An organisation requiring that inference traffic reach an approved endpoint, or
refusing to have models reached through somebody's personal cloud account, would otherwise have no way
to say so.

**Only these names may be pinned**, being the ones that decide where a request goes:
`BRAVE_AI_CHAT_ENDPOINT`, `BRAVE_AI_CHAT_PREMIUM_ENDPOINT`, `BRAVEBOT_USE_BEDROCK`, `AWS_REGION`,
`AWS_PROFILE`, the three `ANTHROPIC_DEFAULT_*_MODEL` tiers, and the `provider` block. Every other name
in the file decides nothing, the signing key and key id included. A name it does not pin resolves
exactly as it would with no such file.

A layer that can pin a preference is a layer somebody uses to pin one. What two parties have a
legitimate say in is where a request goes and whose account pays for it; which theme is on and which
keys do what are neither.

**No credential is read from this file.** A gateway entry's `apiKey` is dropped and the entry's host,
models and variable names are honoured without it. Everyone on the machine can read this file, so a
token in it is a token handed to every account rather than one held by its owner. The service says what
is missing on the first request.

**The `provider` block is pinned whole** rather than a name at a time, because pinning an endpoint
pins nothing while anybody may add a destination beside it. A block that is present and empty says
there are no gateways. A file without the block, or one spelling it as anything but a block, leaves the
gateways a person configured in force: taking every gateway on the machine away on the strength of a
stray `null` is the one reading nobody would intend.

Refusing every account but the organisation's therefore takes **both** halves, the switch pinned off
and the `provider` block pinned, since a gateway entry can name an AWS account too.

The authority here is the filesystem's rather than this program's. Nothing checks who owns the file or
what its permissions are: somebody who can write that path can replace the binary. It fails softly like
any other layer, so a file that is missing, over 64 KB or unparseable pins nothing, and a blank or
non-string value pins nothing under that name.

## Reaching a model through AWS Bedrock

Set these variables to reach models through your own AWS account:

| Variable | What it sets |
|---|---|
| `BRAVEBOT_USE_BEDROCK` | turns the backend on |
| `AWS_REGION` | which region to reach Bedrock in (**required** once it is on) |
| `AWS_PROFILE` | which profile names the credentials to sign with (optional) |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | the model the Opus tier names |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | the model the Sonnet tier names |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | the model the Haiku tier names |

Three services can answer a request: the aichat endpoint Brave runs, AWS Bedrock through your own
AWS account, and an [OpenAI-compatible gateway](#reaching-an-openai-compatible-gateway) you
configured. Every build can reach Brave; the other two are what you configure.

Each tier takes either a model id or an inference-profile ARN. With `AWS_PROFILE` unset the AWS CLI
resolves credentials as it would for any other command, which is what a machine on instance
credentials already relies on.

**Any model your account can reach, whoever makes it.** A request is built in the body Bedrock
states for every provider it hosts rather than in one provider's own, so a tier can name a Claude,
an OpenAI, a Nova or a Llama model, or an inference profile standing for one, and nothing here has
to work out which provider is behind it. Nothing local checks the name you set, and Bedrock refuses
one your account cannot reach.

**The tier words stay `opus`, `sonnet` and `haiku`.** They name a slot in your configuration rather
than a model family, so a tier is whichever model you pointed it at.

**A tier you do not name is left out rather than guessed at.** An ARN cannot be derived from a model
name. Set one tier and one tier is offered.

**Configuring Bedrock takes nothing away from Brave.** Both rosters are offered together in `/model`,
so this adds models rather than replacing them. It also does not move the default: what answers when
nobody has chosen stays what it was.

**The model names the service.** A request goes to whichever service offers the model it names, and
nothing else participates: not which configuration is present, not which service answered last.
Bedrock refuses a model it does not recognise rather than substituting one, and the aichat endpoint has
never heard of an inference-profile ARN.

Your tiers sit under a heading reading `Bedrock, your my-profile AWS profile`, or
`Bedrock, your AWS account` with no profile set. The profile is named because it is what decides which
credentials sign the request, and because Brave serves part of its own roster through Bedrock too.
Every configured tier is marked free: premium means a Leo subscription, and reaching a model through
your own account does not involve one.

There is no automatic entry among them. On the Brave roster it means "let the server choose", which
Bedrock does not offer. A request names one model and gets it or an error.

If one service cannot say what it offers, the models known from your configuration alone are still
offered; a choice is refused only when nothing is left to choose. That is the position somebody
offline is most likely to be in.

### Naming more than three models

The tier variables name three models. A `provider` block keyed `amazon-bedrock` names as many as
your file lists, each under the id a request sends:

```json
{
  "provider": {
    "amazon-bedrock": {
      "options": { "region": "us-west-2", "profile": "my-profile" },
      "models": {
        "openai.gpt-5.6-sol": {},
        "arn:aws:bedrock:us-west-2:…:application-inference-profile/abc": {
          "name": "Sol on Bedrock",
          "limit": { "context": 1050000, "output": 128000 }
        }
      }
    }
  }
}
```

`options.region` is **required**, for the reason `AWS_REGION` is: a guessed region is a request that
fails somewhere far from the mistake. An entry without one configures no service at all.
`options.profile` picks which credentials sign, exactly as `AWS_PROFILE` does, and is optional on the
same terms.

**There is no credential to name here.** Bedrock takes a signature over the request rather than a
bearer token, so this entry reads neither `env` nor `options.apiKey`. Which AWS credentials sign
comes from the profile, resolved at the moment a request needs it.

**This adds to the tier variables rather than replacing them.** A name either of them offers reaches
your account, and everything else on the page above holds unchanged: the models are offered in
`/model` alongside Brave's roster, the default does not move, and each is marked free.

A model named this way has no tier, so its picker row carries the `name` you gave it and falls back
to the id where you gave none. Write one: an inference-profile ARN is not a name anybody reads. A
model named here is also chosen by that id exactly as written, without the gateway's
[`id/name` prefix](#naming-one) in front of it.

`limit.context` states that model's window, which is worth setting here because the figure otherwise
assumed is [deliberately low](#the-assumed-context-window). Following opencode, it needs `output`
beside it or it is not read, and `output` is itself
[how far a reply may run](#how-long-a-reply-may-run).

### Signing in

Where AWS has no usable session, the sign-in happens **before the turn starts**, and only for the
service the next request will actually go to. A turn served entirely by Brave never stops to
authenticate against AWS. The URL and code the AWS CLI prints appear line by line where you are
already reading, because collected up and printed at the end they would arrive once the code had
stopped working.

Credentials are resolved by running the AWS CLI, which is the tool you already sign in with. It holds
short-lived keys that expire during a session. `aws sso logout` clears them, and it takes no option to
narrow itself: it removes every cached token, so other tools sharing that cache need a fresh
`aws sso login` afterwards.

### The assumed context window

Every configured tier is assumed to have a 131,072-token window. Nothing at AWS reports a context
window, and an inference-profile ARN does not say which model it resolves to, so one deliberately low
figure stands in for all of them. Being wrong upward would stop the shortening of a conversation
altogether: every round asks, no round qualifies, and the session runs to exhaustion. Set
`BRAVEBOT_CONTEXT_BUDGET` if you know your model's real window and want to use it, or state it per
model with [`limit.context`](#naming-more-than-three-models).

## Reaching an OpenAI-compatible gateway

A `provider` block names a gateway, the models it offers and where its credential lives. An entry
keyed `amazon-bedrock` names [an AWS account](#naming-more-than-three-models) instead, which is
reached by signing rather than by a token; everything below is about the other entries. The models
a gateway ends up with are offered in `/model` beside Brave's roster and any AWS tiers. It takes nothing away
from those rosters and does not move the default: what answers when nobody has chosen stays what it
was, and the conversation budget stays where it was too.

```json
{
  "provider": {
    "openrouter": {
      "name": "OpenRouter",
      "env": ["OPENROUTER_API_KEY"],
      "models": {
        "z-ai/glm-4.6": { "limit": { "context": 200000, "output": 8192 } }
      }
    }
  }
}
```

| Where | Field | What it holds |
|---|---|---|
| the key under `provider` | | the gateway's id, which is also what a picker row names it by |
| the entry | `name` | something friendlier to show than the id |
| | `env` | variable names that may hold the bearer token, tried in order |
| | `models` | the models to offer, keyed by the name the gateway knows each by |
| `options` | `baseURL` | where requests go |
| | `apiKey` | a token written into the file directly |
| a model | `limit.context` | that model's context window, in prompt tokens |
| | `options` | anything extra to put in the request body |

**The block is opencode's, field for field**, so one copied out of `opencode.json` works unedited.
Nothing is required that opencode does not require, and a field bravebot does not know is read past
rather than refused. opencode's `cost`, `modality` and `package` fields do nothing here.

### Where the requests go

**A gateway bravebot already knows an endpoint for needs no `baseURL`.** `openrouter` is the name it
knows today. Any other id needs one written down, and an entry with neither a known name nor a stated
endpoint configures no service. A stated `baseURL` always wins, so a known name stays usable against a
proxy or a private deployment.

The names it knows are compiled in, and nothing is fetched to resolve one. This value is where a
bearer credential gets sent, so a service that could decide it could redirect your token by answering
a request.

### The credential

Name a variable in `env` and keep the token wherever you already keep secrets. `options.apiKey` is
read too, because it is opencode's field, but a variable wins where both are present. A long-lived
token in a settings file is a token in a file people paste into issues.

It is read at the point a request needs it rather than once at startup, so exporting a new one takes
effect in a session already open. A block that names somewhere for a credential to live and finds
nothing there is a stale or missing token, and its requests are refused with the remedy named rather
than sent. `bravebot doctor` says whether a credential was found, and never what it was. It
prints an `ends` line for the block too, naming the host the token is presented to: that host
is the only surface that revokes it, and deleting the value from this file, or unsetting the
variable, ends this machine's custody and leaves the token live there.

**A block naming no credential at all is a different statement, and a supported one.** No `env` and no
`options.apiKey` is you saying this gateway wants none: its requests carry no `authorization` header
and its roster is asked for without one. `doctor` reports it as needing none rather than as missing
one. Deciding this by endpoint instead would refuse the same local service reached across a LAN or
through a reverse proxy, and a dummy `apiKey` would just teach people to write fake credentials into a
file they paste into issues.

### A local Ollama, or another gateway that wants no key

Ollama wants no API key, so its block names none:

```json
{
  "provider": {
    "ollama": {
      "name": "Ollama (local)",
      "options": { "baseURL": "http://localhost:11434/v1" }
    }
  },
  "model": "ollama/qwen3-coder:30b"
}
```

`baseURL` is written down because `ollama` is not one of the names an endpoint is
[compiled in](#where-the-requests-go) for. There is no `models` key, so Ollama is asked what it has
pulled and `/model` lists what came back.

### Which models are offered

**A block that lists `models` is taken at its word**, in the order you wrote them, and costs no round
trip. That is what keeps a configured gateway working with no network, and is the way to pin a short
list out of a service offering hundreds.

**A block that lists none has the gateway asked.** That is the ordinary case rather than a mistake:
opencode resolves its roster from a registry it fetches, so the commonest block copied out of it names
a credential and nothing else. What your credential may reach is asked for first, and the service's
full catalogue answers only where a gateway does not offer the narrower question. Models that cannot
call tools are left out.

Nothing is capped. Ordering does that work instead: the model a session would use comes first and the
rest are sorted by name. A listing that cannot be fetched contributes nothing and takes nothing away
from the rest of the roster.

### Naming one

Where one model is reachable through more than one service, put the gateway's id in front of the name
to say which you mean:

```
openrouter/z-ai/glm-4.6
```

The name is split once, at the first slash, because most gateway names contain one. The id picks the
service and only the remainder is sent, the id being bravebot's own filing that no gateway has heard
of. A bare name your block lists still finds its gateway, so a choice already recorded by `/model`
keeps working.

### The context window

`limit.context` is optional. A model that states none is assumed to have 131,072 prompt tokens, the
same deliberately low figure a Bedrock tier gets and for the same reason: a budget above the real
window does not compact a conversation late, it stops compacting it at all. A window a gateway reports
is taken where the file stated none; a figure in the file outranks it. Following opencode, `limit`
needs `output` alongside `context` or it is not a `limit` and its figure is not read. `output` states
[how far a reply may run](#how-long-a-reply-may-run).

### What a model's `options` can and cannot do

Whatever you put there reaches the request body as it stands. Nothing parses it, knows what any of its
fields mean, or validates them, so a misspelled routing field is a request the gateway rejects, or
worse one it silently routes somewhere you did not intend.

It cannot replace what the turn itself built. The settings file names a destination, not what was
asked.

## Context budget

A conversation is compacted when it grows past its token budget: an older stretch of it is replaced by
a summary, in the request only.

**The budget is the window the model advertises.** The model listing reports a figure per model, and
that figure is the budget for whichever model you chose. You do not normally set this at all.

A budget you set by hand outranks the advertised one. The built-in default of 24,000 prompt tokens
only stands in for two cases: the automatic entry, whose model is resolved per request so no single
window describes it, and a model that advertises nothing.

An advertised figure is believed even where it is small, and never raised. A budget that makes no
sense falls back to the default rather than disabling compaction, so a misconfiguration cannot
quietly turn the mechanism off.

While the default is standing in, the reading under the input box is
[marked as approximate](../using/interactive-mode.md#looking-up-the-keys), because a conversation
that reads as full against a guess may only mean the guess is too small.

The window is looked up whenever a model is in force, not only when you pick one in the picker, so a
session starting on a model you chose earlier asks again. If that lookup fails the default stays in
place and nothing is said.

Set it by hand when you want a shorter conversation than your model would allow:

```sh
BRAVEBOT_CONTEXT_BUDGET=120000 bravebot
```

The figure compared is what the server said the **last** round's request came to, so the check is one
round late by construction and the budget has to sit below the window rather than at it. A turn that
has not measured anything yet compacts nothing.

`/compact` asks for the same work on demand, at any size, and does not consult the budget. See
[Sessions](../using/sessions.md#long-conversations).

A one-shot run adopts the advertised window too, so a script gets the same budget a session would
rather than falling back to the default.

## How long a reply may run

A Bedrock request states a ceiling on the reply, and a model that states none is assumed to allow
**8,192** tokens. That figure is deliberately low, for the reason the context window's is, with the
asymmetry the other way round: a ceiling below what a model allows costs the tail of a long answer,
while one above what it allows is a request the service refuses outright and refuses every time. A
guess upward would not cost a reply its ending, it would cost the model the ability to answer at all.

Raise it for every model this build reaches:

```sh
BRAVEBOT_OUTPUT_BUDGET=48000 bravebot
```

Or state it for one model, out of the same `limit` block its context window comes from:

```json
{ "limit": { "context": 200000, "output": 32000 } }
```

An exported figure outranks every stated one. The variable exists because the three tier words have no
block to state anything in, and they are how most people reach Bedrock.

Nothing is asked over the network to find this out, and nobody has to supply it. **The Brave endpoint
states no ceiling at all**, so none of this applies there: whatever bounds a reply belongs to the
service.

**A reply the ceiling stopped is kept for what it wrote.** It comes back marked as having stopped
short, with its usage, because everything written before the cutoff is the turn's work. Its tool calls
are not kept, whatever the service sent: a cutoff lands wherever the model happened to be, so arguments
that stopped mid-string are not arguments, and the round it ends is the last one. A reply that reached
the ceiling having written nothing is a failure, and the failure names the ceiling.

## Building with different configuration

A source build captures whatever is set at build time, so the resulting binary works in any directory
rather than needing the environment wherever it is started. A build with nothing set **fails** rather
than producing a binary that only works in the tree it came from. See [Development](../development.md).
