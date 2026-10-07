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

**If Claude Code or opencode already reaches a service on this machine, or Ollama is running on it,**
the first run offers to copy that setup instead, showing everything it would write before it asks.
See [Importing from Claude Code, opencode or Ollama](#importing-from-claude-code-opencode-or-ollama).

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
| `~/.bravebot/agents/<name>.md` | [delegate definitions](agents.md) available in every project |
| `~/.bravebot/sessions/<directory>/` | session records and audit trails |
| `~/.bravebot/lsp/<workspace>/` | a language server's index, one per workspace ([`lsp`](../reference/tools.md#the-index-is-cached-and-it-is-not-small)) |
| `~/.bravebot/findings/<workspace>.jsonl` | the credentials a turn's own writes were found to hold, one per workspace |
| `~/.bravebot/granted/<directory>.jsonl` | the permission rules a checkout proposed that you granted, one file per directory ([`permissions`](#permissions)) |
| `~/.bravebot/trusted/<directory>.jsonl` | the answer to the startup question you pressed `r` for, one per directory ([remembering the answer](../security/trust.md#remembering-the-answer)) |
| `~/.bravebot/untrusted/<directory>.jsonl` | the [delegate definitions' memories](agents.md#memory) a write left untrusted, one per directory |
| `~/.bravebot/history` | prompts you have sent |
| `~/.bravebot/model` | the model chosen with `/model` |
| `~/.bravebot/effort` | the effort level chosen with `/effort` |
| `~/.bravebot/theme` | the theme chosen with `/theme` |
| `~/.bravebot/editor-mode` | the editing style chosen with `/config` |
| `~/.bravebot/panel` | whether the [info panel](../using/sessions.md#telling-sessions-apart) was left open |
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
today. Google Vertex AI is the exception, having no listing to ask (see [Reaching an
OpenAI-compatible gateway](#reaching-an-openai-compatible-gateway)). The choice is written to
`~/.bravebot/model`, so it outlives the session that made it and applies in every directory, except
where the file `--settings` names has a [`model`](#model) key. A checkout's own settings cannot name a
model. A one-shot run reads the same record, so a script uses the model you picked
unless [`--model`](../reference/cli.md#--model-name) names another. A model your AWS account named
for a tier is written as that tier's word, `opus`, `sonnet` or `haiku`, so the record names whatever
the tier variable names when the ARN is replaced.

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
written to `~/.bravebot/effort`, so it outlives the session and applies in every directory whose own
settings name no [`effort`](#effort), and `/status` reports it beside the model.

**Nothing infers a level.** Until you choose one the request carries no such field at all and each
service applies its own default. Taking the row for no level removes the record rather than writing an
empty one, which puts you back where you were before you ever chose. A word bravebot does not define
changes nothing and says so.

The picker is not the only route: the [`effort`](#effort) key names a level in a settings file, which
is how a project asks for one and how an unattended machine has one at all, and
[`--effort`](../reference/cli.md#--effort-level) names one for a single one-shot run.

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

A level you wrote into a model's own [`options`](providers/openai-compatible.md#what-a-models-options-can-and-cannot-do) is not
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
without rebuilding it. The one exception is `BRAVEBOT_DEFAULT_MODEL`, which a
[`model`](#model) key outranks.

| Variable | What it sets |
|---|---|
| `BRAVE_AI_CHAT_ENDPOINT` | the host requests go to |
| `BRAVE_AI_CHAT_PREMIUM_ENDPOINT` | the premium host, used once a subscription is imported |
| `SERVICES_KEY_AICHAT` | the services key requests are signed with |
| `BRAVE_SERVICES_KEY_ID` | the key id that goes with it |
| `BRAVEBOT_DEFAULT_MODEL` | the model to request when no settings file or `/model` choice names one |
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

`BRAVEBOT_DEFAULT_MODEL` is a **default rather than the setting**: a [`model`](#model) key in any
settings file and a choice made with `/model` both win over it, so this applies until somebody names
one.

`BRAVEBOT_CONTEXT_BUDGET` is never baked into a binary. It is a knob one person turns while working,
so it has to be set in the environment.

**Thirteen names can also go in a settings file's [`env`](#settingsjson) block**, under the same
spelling: seven of the nine above, plus the six AWS ones. The two exceptions are `BRAVEBOT_LOCALE` and
`BRAVEBOT_SUBPROCESS_ENV_SCRUB`, which are read from the environment alone. Exporting a name wins over
the file, except where an [administrator pinned it](#pinned-by-an-administrator), and except for
`BRAVEBOT_DEFAULT_MODEL`, which the top-level [`model`](#model) key outranks.

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
| `env`, `attribution`, `keybindings` | per name one level down; the value under a name is replaced whole |
| `run.scrubEnv`, `permissions.deny`, `permissions.ask`, `permissions.additionalDirectories`, `mcp.request` | every file's entries are kept |
| `permissions.allow` | your own file's entries, a `--settings` file outside the project, and a project's entries you granted |
| `provider`, `model`, `advisorModel`, `fallbackModel` | your own file and the file `--settings` names. A project or local file naming any of them is ignored and reported |
| `run.network` | your own file and the file `--settings` names may set either word; a project or local file may set `closed` and never `open`, and is reported when it tried |
| anything else | the closest file that set it wins |

A file that writes `permissions` or `run` as something other than an object, or `run.scrubEnv` or a
list under `permissions` as something other than an array, sets nothing there: the broader files'
block or list stays in force. A `{"permissions": null}` in a checkout therefore cannot clear the
`deny` and `ask` rules in your own file.

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

These keys are read, and anything else in the file is ignored rather than refused, and named by
`bravebot doctor` with the file that set it:

| Key | What it holds |
|---|---|
| `model` | the model to request when nobody has chosen one ([below](#model)) |
| `advisorModel` | the model the planner may consult through the `advisor` tool ([below](#advisormodel)) |
| `fallbackModel` | the model a turn moves to when its own keeps failing ([below](#fallbackmodel)) |
| `effort` | how hard the model is asked to think when nobody has chosen ([below](#effort)) |
| `promptCacheTtl` | how long a gateway or an AWS account keeps a cached prompt, `5m` or `1h` ([below](#promptcachettl)) |
| `editorMode` | whether the input box edits the ordinary way or vi's ([below](#editormode)) |
| `env` | variables, in Claude Code's own shape |
| `permissions` | which actions to refuse, and which to ask about ([below](#permissions)) |
| `provider` | an OpenAI-compatible gateway ([below](#reaching-an-openai-compatible-gateway)), or an AWS account ([below](providers/bedrock.md#naming-more-than-three-models)) |
| `run.scrubEnv` | further variables to keep from a program the agent runs ([below](#runscrubenv)) |
| `run.network` | `open` (the default) or `closed`: whether a program the agent runs keeps the network ([below](#runnetwork)) |
| `run.maxOutput` | how much of what a command printed the agent reads ([below](#runmaxoutput)) |
| `run.defaultSeconds`, `run.maxSeconds` | how long a command may run ([below](#rundefaultseconds-and-runmaxseconds)) |
| `attribution` | what a commit message or a pull request this agent writes may carry ([below](#attribution)) |
| `keybindings` | keys rebound to your own choice ([below](#keybindings)) |
| `search` | how large a tree a search may walk ([below](#search)) |
| `terminalTitle` | whether the terminal's title is set to the session's name ([below](#terminaltitle)) |
| `tui.wheelRows` | how many rows one mouse wheel notch scrolls ([below](#tuiwheelrows)) |
| `updateCheck` | whether startup checks for a newer release ([below](#updatecheck)) |
| `vetting` | whether quarantined content is checked without asking you ([below](#vetting)) |

In `env`, only string values: a number or a boolean is skipped rather than coerced, so write `"1"` and
`"true"`. Every name in the block is read rather than a chosen subset.

A key beside those configures nothing and restricts nothing, which is worth saying plainly for
`sandbox` and `hooks`: a block pasted from another tool's file reads as a restriction in force and is
not one here. `doctor` names the key and the file, and never what the key was set to.

**The file is the same shape as Claude Code's `~/.claude/settings.json`**, so a block that configures
one largely configures the other unedited. The three files resolve in the same order Claude Code's do,
down to the name `settings.local.json`, so knowing where to put a value for one tool is knowing it for
the other. Where a variable names **your** deployment the spelling is kept, which is why the Bedrock
tiers below are `ANTHROPIC_DEFAULT_*_MODEL`. The switch that decides which backend bravebot itself
uses is `BRAVEBOT_USE_BEDROCK`.

:::caution
`BRAVEBOT_USE_BEDROCK` was called `CLAUDE_CODE_USE_BEDROCK`. The old name now **reads as unset**, so a
file or a profile still setting it falls back to the Brave backend without an error. Rename it.

`BRAVEBOT_DEFAULT_MODEL` was called `BRAVE_AI_CHAT_DEFAULT_MODEL`. The old name also reads as unset, so
a run asks for the model the build was made with (or `automatic-bravebot`) until you rename it.
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
**A `.bravebot/settings.json` arrives with a checkout.** A repository you have just cloned cannot name
the host every request goes to, the model, the advisor model, the fallback model, or the environment variables read as a
gateway's credential: a `provider` block, or a `model`, `advisorModel` or `fallbackModel` key, in a project or local file is ignored, and `bravebot
doctor` names each file whose block or key was dropped. It can still set the other keys here, so read
a project's settings file before working in it; `doctor` names the files in force. It cannot grant a
capability either. The names that would (`permissions.allow`, and `permissions.additionalDirectories`)
do not take effect on being read: each is a question you answer when the session opens, the rules in
one box listing them and the directories one box apiece.
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

[`vetting`](#vetting) decides whether you are asked something at all, which is why it is read from
your home file alone and never from a checkout's. [`provider`](#reaching-an-openai-compatible-gateway)
and [`model`](#model) are too, since they decide which host receives your conversation and which
variables are sent to it as a credential, and so is [`advisorModel`](#advisormodel), which decides
which model is sent the whole conversation. [`fallbackModel`](#fallbackmodel) is too, for the same reason.
:::

### `model`

```json
{ "model": "sonnet" }
```

The model to request. This is the one key in the file that **outranks the model baked into the
binary**, and it outranks an exported `BRAVEBOT_DEFAULT_MODEL` too, since that variable names a
default.

**The key is read from `~/.bravebot/settings.json` and from the file `--settings` names, and from no
other.** A `.bravebot/settings.json` or `.bravebot/settings.local.json` that names a model is ignored
and reported by `bravebot doctor`, so a repository you cloned cannot change which model receives your
conversation.

A choice recorded by `/model` wins over the key in `~/.bravebot/settings.json`, and loses to one in
the file `--settings` names.
A pick that no configured service serves, such as an ARN whose profile was replaced, gives way to
the model beneath it instead, and each start says which pick it ignored until you pick another.
[`--model`](../reference/cli.md#--model-name) on a one-shot run, and a model picked in the session
that is running, win over everything. A key left blank names nothing, so a pick still wins over it,
but it hides the key in a file below it: with nothing picked, the exported variable or the build
answers.

`opus`, `sonnet` and `haiku` name a **tier** rather than a model, since that is what a settings file
written for another tool puts here. Each resolves to something reachable: the model your AWS account
named for that tier, and otherwise that tier's name on the Brave roster. A tier word is never sent as
written, because a service has never heard of it. Any other name is used exactly as you wrote it.
Bedrock refuses a model it does not recognise, and the aichat endpoint silently resets one to
`automatic-bravebot`, which is the key appearing to work while changing nothing.

### `advisorModel`

```json
{ "advisorModel": "opus" }
```

The model the planner may consult through the [`advisor`](../reference/tools.md#advisor) tool. With
the key set, every session that has a planner is offered the tool. Without it, and without
[`--advisor`](../reference/cli.md#--advisor-name), there is none. `--advisor` wins over the key for
the run it is given to. A delegate is never offered the tool, and a `--mode manifest` run has no
planner to offer it to.

**The key is read from `~/.bravebot/settings.json` and from the file `--settings` names, and from no
other.** The advisor is sent the whole conversation, so a `.bravebot/settings.json` or
`.bravebot/settings.local.json` that names one is ignored and reported by `bravebot doctor`, as a
`model` is.

The name is read as [`model`](#model) is, so `opus`, `sonnet` and `haiku` name a tier. A model that
your administrator refuses, or that nothing is configured to serve, is not caught when the session
starts: the call that would have asked it is answered with a failure the planner carries on from.

### `fallbackModel`

```json
{ "fallbackModel": "sonnet" }
```

The model a turn moves to when the one it is running on keeps failing. A request that the service
answers with a rate limit or an unavailable status, after the retries bravebot already makes, is
sent again to this model, and the rest of that turn runs on it. bravebot says so when it moves. A
failure of any other kind, such as a refused credential or a request the service rejected, ends
the turn as it did before.

The fallback has to be served by the same service as the model it replaces: the same AWS account,
the same gateway, or the Brave endpoint. A fallback on another service is not used, since that
would send your conversation to a provider you did not choose for it. Delegates and a model your
definition named do not fall back.

**The key is read from `~/.bravebot/settings.json` and from the file `--settings` names, and from no
other.** A `.bravebot/settings.json` or `.bravebot/settings.local.json` that names one is ignored and
reported by `bravebot doctor`. A model your administrator refuses is not used as a fallback.

The name is read as [`model`](#model) is, so `opus`, `sonnet` and `haiku` name a tier.

### `effort`

```json
{ "effort": "high" }
```

How hard the model is asked to think, in the words
[`/effort`](../reference/commands.md#effort-level) takes: `low`, `medium`, `high`, `xhigh` and `max`,
read whatever their case. A word bravebot does not define is no level at all, so nothing you mistype
reaches a request; the request carries no such field and the service applies its own default.

A level recorded by `/effort` ranks as a recorded model does: it wins over the key in
`~/.bravebot/settings.json`, and loses to one in `.bravebot/settings.json`,
`.bravebot/settings.local.json` or the file `--settings` names. A word there that is no level still
wins, and the run asks for none. A blank or a value that is not a string names nothing, so the
record answers, but it hides the key in a file below it: with nothing recorded, the run asks for no
level. [`--effort`](../reference/cli.md#--effort-level) on a one-shot run, and a level
picked in the session that is running, win over everything. Nothing bakes a level in and no
variable names one.

This is the way a **project** can ask for a level, and the way a machine where nobody ever opens the
interactive interface gets one at all: `bravebot -p` and `bravebot --plain` read the key just as the
interface does. A record is stored once per person, so without this two checkouts cannot ask for
different levels.

Everything under [Choosing how hard to think](#choosing-how-hard-to-think) still applies, including
the two cases worth knowing: the models that read no level, and the Brave endpoint, which accepts one
and discards it. Taking the row for no level in the picker removes the record rather than writing an
empty one, so your own file's level answers again in the next session; unset it there if you want
none.

### `promptCacheTtl`

```json
{ "promptCacheTtl": "1h" }
```

How long the service keeps the cached prompt after the last request that read it: `5m` for five
minutes or `1h` for one hour. It applies to requests that go to an AWS account and to an
OpenAI-compatible gateway, and not to Brave's own endpoint. A person who pauses for longer than the
default lifetime between turns pays to read the whole conversation again; the one-hour lifetime avoids
that, and the providers charge a higher rate to write it. With the key absent, bravebot sends no
lifetime and every service keeps its own default. Any other word names nothing and sends nothing.

A model that does not accept a lifetime refuses the request on it. That request is sent again with the
lifetime removed and the cache breakpoints kept, and the model is not sent one again in that process.
A model that does not accept breakpoints at all is handled as before.

`/status` shows the lifetime the setting asks for under the cache figures of the last turn. It is the
setting and not a measurement, so it reads the same after a model has refused the lifetime.

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

### `terminalTitle`

```json
{ "terminalTitle": false }
```

The terminal's title names the session, as `bravebot · dependency audit`
([naming a session](../using/sessions.md#naming-a-session)). `false` leaves the title alone, for a
terminal or multiplexer that manages titles itself. Only the boolean `false` turns it off: `"false"`
in quotes, or any other value, leaves it on. An incognito session leaves the title alone whatever this
says.

### `tui.wheelRows`

```json
{ "tui": { "wheelRows": 5 } }
```

How many rows of the transcript one mouse wheel notch scrolls, in the session view and in
[the scroller](../using/transcript.md) alike. Without this key it is 3, which is what a terminal's
own scrollback moves.

Terminals disagree about how many events a notch sends, and a trackpad swipe sends a stream of them,
so the same figure crawls on one and jumps a screen on another. Raise it where the transcript barely
moves, lower it where one swipe overshoots what you were reading.

Whole numbers from 1 to 100. A larger figure is held to 100 rather than refused, and a zero, a
fraction, a negative or a word leaves the built-in 3 in force. The value is read when the session
opens, so editing it describes your next session.

Unlike `model` and `provider`, this key is read from every settings file, so a checkout's
`.bravebot/settings.json` can name it. It moves a view on your own screen and decides nothing about
where a request goes or what is read, which is why it is not one of the keys held to your home
directory.

### `updateCheck`

```json
{ "updateCheck": false }
```

At startup bravebot says when a newer release is out, from an answer it asks the npm registry or the
GitHub releases API for at most once an hour. `false` turns that off: nothing is read, requested or
recorded, and no line is said. Setting the environment variable `BRAVEBOT_UPDATE_CHECK=0` does the
same, for a machine where updates are managed elsewhere, and either one is enough. Only the boolean
`false` and the value `0` turn it off. It does not affect the installer's checksum and signature
checks.

### `run.scrubEnv`

```json
{ "run": { "scrubEnv": ["MY_TOKEN", "OTHER_SECRET"] } }
```

Variables to withhold from a program the agent runs, on top of bravebot's own credentials, which are
withheld with no configuration at all. See [`run`](../reference/run-tool.md#what-a-program-is-handed).

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

### `run.network`

```json
{ "run": { "network": "closed" } }
```

`open` is the default: every program the agent runs keeps the network. `closed` takes it from every
program except those that carry a reason to have it: a package manager that fetches (`cargo`, `npm`,
`pip`, `go`, `mvn`, `gradle`), a `git` or `gh` command that talks to a remote, `curl`, `ssh`, and
a command that was lent a credential (remote, cloud, cluster or container). A program the agent could have written itself, under a directory it may write to, gets nothing by being named `curl`. `cat`, `grep`, `make`, `python3` and a test
run get none, so what they read cannot be sent anywhere.

`--run-network closed` sets it for one run, and a machine-level file can pin it, which no other
layer overrides. A project's own settings can close the network and cannot open it. On Linux the
operating system cannot take the network from one program and leave it to another, so under
`closed` a program that does not need it is not started, and the result says so. The opening screen,
`/status` and `bravebot doctor` say when it is closed and which layer closed it.

### `run.maxOutput`

```json
{ "run": { "maxOutput": 65536 } }
```

How many bytes of what a command printed reach the agent. Past it, the output is cut in the middle:
the beginning and the end are kept, with a line between them saying how much went, because a build
log's verdict is at the end and its first error is near the beginning.

Without this key the figure is 16 KiB. It is there because your conversation has a finite amount of
room and one command that printed a hundred thousand lines could fill it, leaving none for the work.
That is a budget rather than a rule about what is allowed, which is why you can name your own: raise
it when a long test run is worth the room, lower it when you would rather the agent read a summary
and ask.

One number covers a command's output, a `job_output` page of a background job's output, the account
a background job gives when it finishes, and a `read_output` page of output the agent was shown the
beginning and end of. Output you release with `read_output` after being asked is not cut by it: that
reaches the agent whole.

**What was printed is never lost.** Only what reaches the conversation is cut. The whole output stays
beside the sample, so the agent can read the middle a page at a time, hand it to a check or write it
to a file without running the command again.

**Bytes, not characters**, and a cut always lands between characters rather than inside one.

A cap of `0` leaves the built-in figure in force rather than meaning "no output": so does anything
that is not a whole number, so a half-typed file costs you nothing. A delegate runs under whatever
the session that started it is using.

### `run.defaultSeconds` and `run.maxSeconds`

```json
{ "run": { "defaultSeconds": 900, "maxSeconds": 1800 } }
```

How long a command may run. `defaultSeconds` is what a command gets when the agent asks for no
deadline of its own, and `maxSeconds` is the most it may ask for. Without them the two are 300 and
600 seconds.

Raise the first when the thing you ask for most often takes longer than five minutes. A checkout whose
test run takes eight will otherwise be stopped partway through, every time, and the agent learns it
needs a longer deadline by spending a round finding out. Raise the second, and leave the first alone,
when one particular job needs twenty minutes and you still want a program that hangs given up on
quickly. Lower either to keep a session brisk.

**Two keys, because they are two decisions.** One is what every command gets; the other is the most
any command may have. Somebody who wants their build to finish needs the first. Somebody who wants one
long integration run needs the second.

**A figure you did not write never overrides one you did.** Write `defaultSeconds` above 600 and the
ceiling rises to meet it, since a default the ceiling forbids would be a number that does nothing.
Write `maxSeconds` below 300 and the default comes down with it, since a default no command could ask
for is not a default. Write both and the ceiling you wrote is the bound.

A command is never given less than one second, and that figure is not configurable: below it a command
would end at the moment it began.

**Reaching the deadline is not a failure.** The command is stopped and whatever it printed comes back,
which is why a server or a watcher is better started with `background: true` instead: see
[the tools reference](../reference/run-tool.md#a-line-has-a-deadline).

A figure of `0` leaves the built-in one in force rather than meaning "no limit": so does anything that
is not a whole number of seconds. The agent is told both figures, so a ceiling you raise is one it
knows it may ask for, and a delegate runs under whatever the session that started it is using.

### `permissions`

```json
{
  "permissions": {
    "deny": ["Read(.env)", "Edit(src/**)", "Bash(curl *)"],
    "ask": ["Bash(git push *)"],
    "allow": ["Bash(cargo test)", "Bash(ls *)", "WebFetch(domain:docs.rs)"],
    "additionalDirectories": ["../shared-lib"],
    "readsStayInWorkspace": true,
    "bypassUnreachable": true
  }
}
```

The same three lists Claude Code keeps, with the same spellings, so a block copied out of
`~/.claude/settings.json` works unedited. What a rule is allowed to decide, and the reason it may
never trust a command's output, is on
[Approvals and permissions](../security/permissions.md#rules-you-write-down-in-advance).

The last two keys are not rules. Every rule names the thing it refuses, so a path nobody wrote down is
a path no rule covers; these two state a standing refusal instead, and are described under
[two refusals you do not have to enumerate](#two-refusals-you-do-not-have-to-enumerate) below.

`deny` and `ask` work from any of the files. `allow` works from `~/.bravebot/settings.json` and from
a `--settings` file outside your project; a project's own entries are proposed to you in a box when
the session opens, and work once you grant them. See [how the files combine](#settingsjson) for why.

A rule is `Tool` or `Tool(specifier)`, and names one of five **families**:

| Family | Covers |
|---|---|
| `Read` | every tool that reads or enumerates a file |
| `Edit` | every tool that changes one |
| `Bash` | running a program |
| `WebFetch` | fetching a URL |
| `Mcp` | calling a tool of an [MCP server](mcp-servers.md#rules): `Mcp(weather)` or `Mcp(weather:get_alerts)` |

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

On Windows a path on a drive is a full path, with the drive as its first segment:
`Read(//D:/work/secrets/**)` covers `D:\work\secrets\key` whichever separator the path is written
with, and whichever case the drive letter is in. A rule written from the drive letter is the same
rule, though in a JSON file each backslash is doubled, `"Read(D:\\work\\secrets\\**)"`, so the
forward-slash form is easier to write. `~/x` starts at your home directory on its drive in the same
way. `Read(/D:/work/**)`, with one slash, is still about the settings file's directory. A workspace
rule such as `Read(.env)` does not cover a file on a drive outside the workspace.

**A one-segment relative pattern floats where it restricts and not where it grants.** `Edit(src/**)`
in `deny` or `ask` covers a `src` directory at any depth, including a copy under `vendor`; the same
pattern in `allow` covers only the `src` at the top. Anchor it as `Edit(/src/**)` to pin it to one
place in either list.

A **command** specifier matches a step's program and arguments, with `*` standing in for any text:

| Rule | Matches | Does not match |
|---|---|---|
| `Bash(cargo test)` | `cargo test` | `cargo test --release` |
| `Bash(ls *)` | `ls`, `ls -la` | `lsof`, `"ls /x"` |
| `Bash(ls*)` | `ls`, `lsof`, `"ls /x"` | |
| `Bash(* --help *)` | `npm run --help x` | `npm --help` |

A trailing ` *` also matches the bare command, but only when it is the rule's only wildcard. The
space before it is part of the rule. A trailing `:*` is the same rule as a trailing ` *`, and a colon
anywhere else is an ordinary character.

A space in an `allow` rule stands for the gap between two words and nothing else, so `Bash(ls *)` does
not match `"ls /x"`, which is one quoted program word naming a script at `ls /x`. A space in a `deny`
or `ask` rule also matches a space inside a word, so the same rule in those lists still covers it.

So an `allow` rule cannot spell out a word that holds a space: `Bash(python3 my script.py)` names three
words and does not match `python3 "my script.py"`. A `*` in the space's place does, along with anything
else in that place. A `*` covers any text, spaces inside a word included, so `Bash(ls*)` grants every
program whose name begins with `ls`, a script in the checkout among them.

**Every step of a command line is judged on its own**, as its program and arguments, each one word
as the line was split. Restricting any one step restricts the whole line.
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

#### Two refusals you do not have to enumerate

| Key | What `true` does |
|---|---|
| `readsStayInWorkspace` | the file tools refuse every path outside the working directory, in every mode. No directory opens beside the workspace, whatever a rule, a mode, or an answer you give during the session would otherwise open: `/add-dir` and `--add-dir` are refused, and a name in `additionalDirectories` is refused rather than put to you. `/cd` may move further into the tree and not back out |
| `bypassUnreachable` | the mode that asks about nothing is off the Shift-Tab ladder, and `--dangerously-skip-permissions` is refused with the key and the file named rather than ignored |

Both are **off until a file turns one on**, exactly as [`vetting`](#vetting) is: a file naming neither
behaves exactly as one did before the keys existed. `true` is the restrictive answer, and anything that
is not a boolean is absence, so `"true"` as a string, a `1` or a `null` refuses nothing and is reported
by `doctor` rather than obeyed.

**Both are read from every file, and the strictest answer wins.** Your own file, a checkout's, the
machine-local one, a `--settings` file and [the one an administrator pinned](#pinned-by-an-administrator)
may each ask for either, and a file that asks gets it: the usual rule that the closest file wins does
not apply, because neither key can grant anything, so there is nothing for a file read later to lift.
Writing `false` therefore does not switch off what another file asked for. This is why they are not
restricted to `~/.bravebot/settings.json` the way `vetting.auto` is: a checkout asking for one takes
nothing away from you.

`readsStayInWorkspace` governs the file tools, as the path rules do. It does not confine a program
`run` starts. `/cd` may move further into the tree and not back out, so the reach never grows: a
parent holds whatever was refused beside the old directory, and allowing the move back would allow
every move outward. To work above where you are, start a session there. It is read when the session
opens, so one that moves into a checkout asking for it is confined from the next session there.

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

Nine actions can be moved and nothing else can. A chord is spelled `ctrl-x`, `alt-o` or `ctrl+x`:

| Action | Default | What it does |
|---|---|---|
| `background` | `ctrl-b` | move the command a turn is waiting on to the background |
| `editor` | `ctrl-g` | open the current prompt in your editor |
| `watch` | `ctrl-l` | watch a background delegate, or inspect what is running |
| `scroller` | `ctrl-o` | open the [transcript scroller](../using/transcript.md) |
| `history` | `ctrl-r` | search your prompt history |
| `stash` | `ctrl-s` | put the current line aside, or bring it back |
| `trail` | `ctrl-t` | toggle the [audit trail](../security/audit-trail.md) |
| `paste` | `ctrl-v` | paste from the clipboard |
| `panel` | `ctrl-x` | show or hide the [info panel](../using/sessions.md#telling-sessions-apart) |

**A chord has to carry Ctrl or Alt.** Every unmodified key is already answered (a character is typed,
Enter sends, Escape clears, Tab takes what is offered, the arrows move), so handing one to an action
would take it out of the alphabet. `shift-a` and `ctrl-shift-a` name an event a terminal never
reports, and Ctrl-C, Ctrl-D, Ctrl-J and Shift-Enter are refused because leaving and starting a line
are not rebindable.

**Every action keeps a key of its own.** A chord that cannot be read, or that the input box already
answers, leaves that action on its default. So does one two actions both asked for: both fall back
rather than one winning, since which won would come down to the order the file was read in. Two
actions *trading* chords is not a conflict and both get what they asked for. `panel` is the newest
of the nine, so a file that already gave `ctrl-x` to another action now asks for the chord the panel
stands on, and that action is back on its default until the file moves `panel` as well.

The block layers per action the way `env` does, so a project file moving one action says nothing about
the other eight. See [Interactive mode](../using/interactive-mode.md) for what the keys do.

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
**This block is read from `~/.bravebot/settings.json` alone.** A `.bravebot/settings.json` in a
checkout that names it is reported by `doctor` rather than obeyed. This key decides whether you are
asked before content nobody vouched for reaches the planner, so a line in a repository you just cloned
could otherwise turn the asking off for whoever opened it.
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
in the file decides nothing, the signing key and key id included, save the two pairs of lists and the
two refusals: `"mcp": { "allow": [...], "deny": [...] }` name the
[MCP servers](mcp-servers.md#refused-by-an-administrator)
a session on the machine may start and may not, by host or by command. The file can keep a server
from starting and never add one. A name it does not pin resolves exactly as it would with no such
file.

`"models": { "allow": [...], "deny": [...] }` name the models a machine may request and may not, read
on the same footing:

```json
{
  "env": { "BRAVEBOT_USE_BEDROCK": "1", "AWS_PROFILE": "the-org-account" },
  "models": {
    "allow": [
      "arn:aws:bedrock:us-west-2:000000000000:application-inference-profile/approved-opus",
      "arn:aws:bedrock:us-west-2:000000000000:application-inference-profile/approved-sonnet"
    ]
  }
}
```

Pinning the account without this says where the traffic goes and not what it spends: every other model
name still reaches the pinned account and is billed to it. A name here is the model as a request
carries it, an inference-profile ARN or a gateway slug such as `z-ai/glm-4.6`, compared exactly, and a
tier word is checked as the model that tier names. Without an `allow` list every model not denied is
requested as before; with one, a model no entry names is refused, and `"allow": []` refuses every
model. A deny entry wins over an allow entry naming the same model.

This can only refuse. There is nowhere in the file to hold a roster, so an entry permits a name some
service already offers rather than adding one, and the file cannot say which model is the default.
`/model` lists only what the machine may request. A model you had picked that the lists refuse is
ignored, your `~/.bravebot/model` is left as it is, and the session opens on the configured model and
says so; where that model is refused too the start is refused and names the file. An `AGENTS.md`
definition naming a refused model is refused when it is read, when the planner starts it as a delegate
and when you address it. A skill naming a refused model leaves the turn on the session's model and
says which file refused it.

`"permissions": { "readsStayInWorkspace": true, "bypassUnreachable": true }` are read here on the same
reasoning: each can only take capability away, never add any, so keeping the file tools inside the
working directory or putting the mode that asks about nothing out of reach is something two parties can
have a legitimate say in. Neither is pinned in the sense the variables are. A name in the list above is
answered by this file and nothing else is consulted for it; these two take the strictest answer any
file gave, so somebody may ask for either where this file did not, and no file of theirs can lift it
where it did. The rules themselves are not read here: a rule names a path, a program or a host, which
is deciding what a session may work on rather than whether it may leave the tree at all.

A layer that can pin a preference is a layer somebody uses to pin one. What two parties have a
legitimate say in is where a request goes, whose account pays for it, and what a session on the machine
may reach; which theme is on and which keys do what are neither.

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

## Importing from Claude Code, opencode or Ollama

A first run with nothing configured, in a terminal, reads what Claude Code and opencode set up in your
home directory, and asks a running Ollama what it serves, before it names the three routes. Where any
of them reaches a service bravebot can use, it shows what it would add to `~/.bravebot/settings.json`
and asks:

```
Claude Code configures a model service bravebot can use, in /home/you/.claude/settings.json.
Importing it adds these to /home/you/.bravebot/settings.json:
  env.BRAVEBOT_USE_BEDROCK: "1"
  env.AWS_REGION: "us-west-2"
  env.ANTHROPIC_DEFAULT_SONNET_MODEL: "us.anthropic.claude-sonnet-4-5-20250929-v1:0"
  model: "sonnet"
Import this from Claude Code? [y/N]
```

Only a yes writes. Anything else, or the end of input, declines, and the start is refused as before. A
decline is not remembered, so the next start asks again. After a write the settings are read back from
disk, and the session opens on them. Where the model the session would use reads its key from a
variable you have not exported, the start names the variable and stops; an unset variable on any other
imported gateway is named and the session opens anyway. A settings file that changed while you were
answering is not written over.

**What is read.** Claude Code's `settings.json` in `$CLAUDE_CONFIG_DIR` (by default `~/.claude`), and
`CLAUDE_CODE_USE_BEDROCK` if you export it. opencode's `opencode.json` and `opencode.jsonc` in
`$XDG_CONFIG_HOME/opencode` (by default `~/.config/opencode`), the file `OPENCODE_CONFIG` names, and
its `auth.json` in `$XDG_DATA_HOME/opencode`. A checkout's `.claude/`, `opencode.json` and
`.opencode/` are never opened: they hold whatever the repository's author wrote, and would otherwise
decide where your key is sent.

**A running Ollama.** Ollama is asked what it has pulled at the address `OLLAMA_HOST` names, read as
Ollama reads it, or at `http://localhost:11434` where it is unset. Only an address on this machine is
asked; one on another machine is named as left, and nothing is sent to it. A server that answers is
offered as [the block on the gateway page](providers/openai-compatible.md#a-local-ollama-or-another-gateway-that-wants-no-key), with the newest
model that can call tools as `model`:

```
Ollama is running at http://localhost:11434, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "ollama/qwen3-coder-oc:latest"
  provider.ollama, reached at http://localhost:11434/v1: {"name":"Ollama (local)","options":{"baseURL":"http://localhost:11434/v1"}}
Import this from Ollama? [y/N]
```

A model that reports it cannot call tools is never the one written, since every turn calls them; an
Ollama with none that can is named as left. Nothing answering, or an answer that is not Ollama's
listing, says nothing. The request waits a few seconds at most. Where your settings file already has
an entry for a server on the same port, under any name, Ollama is not offered again.

**What can be imported.** A Claude Code Bedrock setup, under bravebot's own names, when a region is
named. An opencode `provider` entry reached through an OpenAI-compatible SDK, with only the fields
bravebot reads, and an `auth.json` API key for a gateway whose endpoint bravebot knows. opencode's
`disabled_providers` and `enabled_providers` are honoured, and a name already set in your settings file,
or pinned by an administrator, is left as it is. A model opencode's `amazon-bedrock` entry lists that
one of your Bedrock tiers already names, such as the Claude Code import's
`ANTHROPIC_DEFAULT_OPUS_MODEL`, is not added to the entry again.

**Keys.** A source that names a variable has only the name written, and opencode's `{env:VAR}` becomes
`"env": ["VAR"]`. A key the source holds itself is asked about on its own question, which names the host
it would be sent to and says it is kept in plain text; the key is never shown. Declining writes the
entry reading the variable opencode reads for it, such as `OPENROUTER_API_KEY`, and says to export it.
A `{file:path}` key is not followed, and neither is a substitution inside a longer value, such as
`Bearer {env:TOKEN}`: that entry is named as left.

**What is never imported**: commands, permissions, hooks and MCP servers. What was found and cannot be
used, such as an Anthropic API key or a Vertex AI setup, is named with the reason before the routes,
and its value is never shown.

A run that cannot ask (`-p`, `--json`, a pipe or a redirect, `doctor`) imports nothing, and its
refusal names the command that asks:

```sh
bravebot import-providers
```

That command asks the same questions at any time, whether or not a service is configured. It refuses
without a terminal and in an incognito session. An incognito session asks no Ollama either, since it
has nothing to write.

## Reaching a model through AWS Bedrock

Set `BRAVEBOT_USE_BEDROCK` and `AWS_REGION` to reach models through your own AWS account, and name
the models with the `ANTHROPIC_DEFAULT_OPUS_MODEL`, `ANTHROPIC_DEFAULT_SONNET_MODEL` and
`ANTHROPIC_DEFAULT_HAIKU_MODEL` variables or a `provider` block keyed `amazon-bedrock`. The models
are offered in `/model` beside Brave's roster. [Reaching a model through AWS Bedrock](providers/bedrock.md)
has the variables, the signing in, and the assumed context window.

## Reaching an OpenAI-compatible gateway

A `provider` block in a settings file names a gateway, the models it offers and the variable that
holds its credential. It is opencode's block, so one copied from `opencode.json` works unedited, and
a local Ollama needs no key. [Reaching an OpenAI-compatible gateway](providers/openai-compatible.md)
has the fields, the credential, which models are offered, and how to name one.

Google Vertex AI is reached by a block keyed `google-vertex`, or with no block by exporting
`GOOGLE_API_KEY` and `GOOGLE_CLOUD_PROJECT`. It has no model listing a key can call, so `/model`
offers a short list of Gemini models built into bravebot, and a block that lists `models` is offered
those instead. A Vertex model, on the built-in list or off it, is named with the service's id in
front, as `--model google-vertex/google/gemini-3-flash-preview` or the same value in the
[`model`](#model) key.
[Google Vertex AI](providers/openai-compatible.md#google-vertex-ai) has the fields.

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

A Bedrock request states a ceiling on the reply, and a model that states none is sent **32,000**
tokens, which every model on Anthropic's current lineup allows. A model that allows less refuses the
request, and is asked again with **8,192**: once per model for as long as bravebot runs, so the
refusal costs one request rather than every answer. A figure you state is never lowered, so a model
that refuses one you chose fails with the service's refusal.

A reply that reaches the ceiling part way through a tool call makes no call. The model is told the
ceiling and what it was writing, and asked once to do the work in smaller parts, such as a long file
written in pieces. If it runs out again the turn ends there. A reply that runs out part way through
its answer ends the turn with what it wrote.

The ceiling is also what Bedrock reserves against the account's tokens-per-minute quota while each
request runs, so on an inference profile a team shares, a larger one means fewer requests at once
before any of them is throttled.

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
