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

**If Claude Code or opencode already reaches a service on this machine, or Ollama, LM Studio or
llama-server is running on it,** the first run offers to copy that setup instead, showing everything
it would write before it asks. See
[Importing from Claude Code, opencode or a local model server](#importing-from-claude-code-opencode-or-a-local-model-server).

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
| `~/.bravebot/rules/<name>.md` | [rules](instructions.md) for every project, in force every turn or once a file their `paths` match is touched |
| `~/.bravebot/sessions/<directory>/` | session records and audit trails, removed after [`retentionDays`](#retentiondays) |
| `~/.bravebot/lsp/<workspace>/` | a language server's index, one per workspace, removed after [`retentionDays`](#retentiondays) ([`lsp`](../reference/tools.md#the-index-is-cached-and-it-is-not-small)) |
| `~/.bravebot/findings/<workspace>.jsonl` | the credentials a turn's own writes were found to hold, one per workspace, removed after [`retentionDays`](#retentiondays) |
| `~/.bravebot/granted/<directory>.jsonl` | the permission rules a checkout proposed that you granted, one file per directory ([`permissions`](#permissions)) |
| `~/.bravebot/trusted/<directory>.jsonl` | the yes to the startup question, one file per directory ([remembering the answer](../security/trust.md#remembering-the-answer)) |
| `~/.bravebot/untrusted/<directory>.jsonl` | the [delegate definitions' memories](agents.md#memory) a write left untrusted, one per directory |
| `~/.bravebot/history` | prompts you have sent |
| `~/.bravebot/model` | the model chosen with `/model` |
| `~/.bravebot/effort` | the effort level chosen with `/effort` |
| `~/.bravebot/theme` | the theme chosen with `/theme` |
| `~/.bravebot/editor-mode` | the editing style chosen with `/config` |
| `~/.bravebot/language` | the interface language chosen with `/language` ([details](language.md)) |
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
followed. That holds for `~/.bravebot` itself, for `logs` and for the `shell` directory the
[shell hook](../reference/cli.md#shell-init) uses: if you link any of them to another volume, the
directory it leads to keeps the mode you gave it. What else is in your home directory is not this
program's business, and somebody keeping
their sessions on another volume has put the target outside what bravebot was given.

A file **you** put there keeps the mode you gave it. `settings.json`, your standing instructions and
your skills are read rather than written, so nothing changes them; the directory's own mode is what
keeps them private.

Every operation here degrades to doing nothing. A missing home directory, a read-only disk or a
corrupt file does not stop a session starting.

Session records, language server indexes and findings files are removed once nothing has written them
for [`retentionDays`](#retentiondays) days, 30 where you state none. What you asked to keep is left
whatever its age: the rules you granted, your answers to the startup question, the prompt history and
every file you wrote yourself.

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

### Moving the state directory

`BRAVEBOT_HOME` names the state directory itself, so two accounts or a CI job can each keep their own
settings, sessions, history, skills and imported subscription without changing `HOME` for everything
else the process starts:

```sh
BRAVEBOT_HOME=/srv/ci/bravebot-state bravebot -p "summarize the failing tests"
```

It answers before `HOME`. What it names is the directory, not a profile directory to put `.bravebot`
under, and the modes described above apply to it as they do to `~/.bravebot`: reachable by you
alone, narrowing stops at the directory you named, and a link out of it is stepped over. Name a
directory bravebot can keep to itself, since it is narrowed to you alone and its files are written
beside whatever is there already; your home directory or a shared directory is a poor choice. A `~`
in a path you write still means your home directory, which `HOME` still names.

A value that is empty, or is not an absolute path, names nothing and is passed over, so the directory
is the one `HOME` names, as though the variable were unset. A relative value would otherwise name a
directory under wherever you happened to start. `bravebot doctor` prints the directory in use and the
variable that named it, which settles which of the two is in force, and says when a value was passed over.

The variable is read from the environment and from nothing else. This directory is trusted for being
yours, and a settings file, which a checkout can supply, cannot move it. `install.sh` reads it too and
records where it installed in that directory. The [shell hook](../reference/cli.md#shell-init) reads it
as your shell sources the hook, so export it before the line in your shell's startup file that
evaluates the hook, and the hook records in that directory.

:::warning
A program a `run` starts is refused `~/.bravebot`, where the gateway keys and the premium token are,
and not a directory `BRAVEBOT_HOME` puts anywhere else
([what a stage cannot reach](../security/security.md)). Keep it somewhere you would not mind a
program a `run` starts reading, or leave the variable unset.
:::

## Choosing a model

`/model` opens a picker on the model in use and writes the choice to `~/.bravebot/model`. See [Choosing a model](model.md).

## Choosing how hard to think

`/effort` opens a picker of five levels, and writes the choice to `~/.bravebot/effort`. See [Choosing how hard to think](effort.md).

## Choosing a theme

`/theme` opens a picker that live-previews over your own transcript, and writes the choice to `~/.bravebot/theme`. See [Choosing a theme](theme.md).

## Choosing a language

The interface is read in your language where a translation has shipped, and in English otherwise. See [Choosing a language](language.md).

### What stays in English

The names of the slash commands, the letters `y` and `n`, the audit trail and, unless a language
supplies its own list, the words on the working indicator. See
[What stays in English](language.md#what-stays-in-english).

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
| `BRAVEBOT_HOME` | the directory bravebot keeps its state in, in place of `~/.bravebot` ([below](#moving-the-state-directory)) |
| `BRAVEBOT_SUBPROCESS_ENV_SCRUB` | `0` hands a program bravebot starts its own credentials ([`run.scrubEnv`](../security/sandbox.md#runscrubenv)) |

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

**Thirteen of these names can go in a settings file's [`env`](#settingsjson) block**, under the same
spelling: seven of the ten above, plus the six AWS ones. The three exceptions are `BRAVEBOT_LOCALE`,
`BRAVEBOT_SUBPROCESS_ENV_SCRUB` and `BRAVEBOT_HOME`, which are read from the environment alone. Exporting a name wins over
the file, except where an [administrator pinned it](managed-settings.md), and except for
`BRAVEBOT_DEFAULT_MODEL`, which the top-level [`model`](#model) key outranks.

Six network names can go there as well: `SSL_CERT_FILE`, `SSL_CERT_DIR`, `ALL_PROXY`, `HTTPS_PROXY`,
`HTTP_PROXY` and `NO_PROXY`. They are read from your own file and a `--settings` file outside the
project, and from no project or local file; see
[certificates and proxies](../security/security.md#certificates-and-proxies).

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
A file [an administrator pinned](managed-settings.md) answers above all of them, and above the
environment too.

A later file overrides an earlier one **a name at a time** rather than wholesale, so a file that sets
one thing leaves everything else in force:

| What | How the files combine |
|---|---|
| `env`, `keybindings` | per name one level down; the value under a name is replaced whole |
| `run.scrubEnv`, `permissions.deny`, `permissions.ask`, `permissions.additionalDirectories`, `mcp.request` | every file's entries are kept |
| `permissions.allow` | your own file's entries, a `--settings` file outside the project, and a project's entries you granted |
| `provider`, `model`, `advisorModel`, `fallbackModel`, `summaryModel`, `planModel`, `processorModel`, `agent`, `style`, `attribution` | your own file and the file `--settings` names. A project or local file naming any of them is ignored and reported, `headers` inside `provider` included |
| `references` | your own file and a `--settings` file outside the project; a project or local file's block, and a `--settings` file that resolves inside the project, is ignored and reported, and a later file's block replaces an earlier one's |
| `run.network` | your own file and the file `--settings` names may set either word; a project or local file may set `closed` and never `open`, and is reported when it tried |
| `sandbox.network.deniedHosts`, `sandbox.network.onUnlisted: refuse` | every file's entries are kept |
| `sandbox.network.allowedHosts`, `sandbox.network.onUnlisted: ask` | your own file and a `--settings` file outside the project; a project or local file's are ignored and reported |
| `sandbox.filesystem.denyRead`, `sandbox.filesystem.denyWrite` | every file's entries are kept |
| `sandbox.filesystem.allowRead`, `sandbox.filesystem.allowWrite` | your own file's entries and a `--settings` file outside the project; a project or local file's are ignored and reported |
| `git.identities` | your own file and a `--settings` file outside the project, the later one's entry for an owner replacing the earlier one's; a project or local file's are ignored and reported |
| `askIdleSeconds` | your own file and a `--settings` file outside the project; a project or local file's is ignored and reported |
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
| `summaryModel` | the model compaction summaries and `/goal` checks run on ([below](#summarymodel)) |
| `planModel` | the model a turn that begins in plan mode runs on ([below](#planmodel)) |
| `processorModel` | the model an isolated processor runs on ([below](#processormodel)) |
| `references` | directories outside the project that the planner may consult, each with a name and a description ([below](#references)) |
| `agent` | the definition every turn of a session is addressed to when `--agent` names none ([below](#agent)) |
| `effort` | how hard the model is asked to think when nobody has chosen ([below](#effort)) |
| `promptCacheTtl` | how long a gateway or an AWS account keeps a cached prompt, `5m` or `1h` ([below](#promptcachettl)) |
| `editorMode` | whether the input box edits the ordinary way or vi's ([below](#editormode)) |
| `language` | the language the interface is in: a locale tag such as `fr`, or `auto`. Read from your own file only ([details](language.md)) |
| `style` | the output style a session starts in: `concise`, `explanatory`, `proactive` or a file in `~/.bravebot/styles`. Read from your own file and the file `--settings` names only, and it cannot name a project's style from `.bravebot/styles` (pick that with `/style`) |
| `limit` | the most tokens or Leo Premium credits a session may spend before it asks ([below](#limit)) |
| `maxRunningDelegates` | the most delegates one turn may have running at once ([below](#maxrunningdelegates)) |
| `env` | variables, in Claude Code's own shape |
| `permissions` | which actions to refuse, and which to ask about ([below](#permissions)) |
| `provider` | an OpenAI-compatible gateway, with any extra request headers it carries ([below](#reaching-an-openai-compatible-gateway)), or an AWS account ([below](providers/bedrock.md#naming-more-than-three-models)) |
| `run.scrubEnv` | further variables to keep from a program the agent runs ([details](../security/sandbox.md#runscrubenv)) |
| `run.network` | `open` (the default) or `closed`: whether a program the agent runs keeps the network ([details](../security/sandbox.md#runnetwork)) |
| `sandbox.network.allowedHosts`, `deniedHosts`, `onUnlisted` | the hosts a program the agent runs may reach ([details](../security/sandbox.md#sandboxnetwork)) |
| `sandbox.filesystem.allowRead`, `denyRead`, `allowWrite`, `denyWrite` | lists of paths that move what a program the agent runs reads and writes ([details](../security/sandbox.md#sandboxfilesystem)) |
| `git.identities` | the ssh key a `git` command offers first for one owner's repositories ([below](#gitidentities)) |
| `run.maxOutput` | how much of what a command printed the agent reads ([below](#runmaxoutput)) |
| `download.maxBytes` | the most one `download_url` call may save to a file ([below](#downloadmaxbytes)) |
| `run.defaultSeconds`, `run.maxSeconds` | how long a command may run ([below](#rundefaultseconds-and-runmaxseconds)) |
| `attribution` | what a commit message or a pull request this agent writes may carry ([below](#attribution)) |
| `keybindings` | keys rebound to your own choice ([below](#keybindings)) |
| `search` | how large a tree a search may walk ([below](#search)) |
| `deferMcpToolsAbove` | the size past which a server's tools are offered by name until one is loaded ([below](#defermcptoolsabove)) |
| `retentionDays` | how many days a session record, a language server index and a findings file are kept ([below](#retentiondays)) |
| `awaySummaryEnabled` | whether a session is recapped by itself after you have been away ([below](#awaysummaryenabled)) |
| `awaySummaryIdleSeconds` | how long after a turn finishes the automatic recap waits ([below](#awaysummaryidleseconds)) |
| `askIdleSeconds` | how long a question from the agent waits with no key pressed before it is closed ([below](#askidleseconds)) |
| `notificationMethod`, `notificationCondition` | how and when the terminal is told a turn ended or a question is waiting ([below](#notificationmethod-and-notificationcondition)) |
| `suggestions` | whether the empty box offers a suggested next prompt ([below](#suggestions)) |
| `terminalTitle` | whether the terminal's title is set to the session's name ([below](#terminaltitle)) |
| `deepLinks` | `false` makes `bravebot --handle-uri` refuse every `bravebot://open` link ([below](#deeplinks)) |
| `tui.mouse` | `false` stops the interface capturing the mouse, so the terminal's own selection works ([below](#tuimouse)) |
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
the host every request goes to, the headers it carries, the model, the advisor model, the fallback model, or the environment variables read as a
gateway's credential: a `provider` block, or a `model`, `advisorModel`, `fallbackModel`, `summaryModel`, `planModel` or `processorModel` key, in a project or local file is ignored, and `bravebot
doctor` names each file whose block or key was dropped. A `headers` block lives inside `provider`, so it is dropped with the rest of the block. It can still set the other keys here, so read
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

### `processorModel`

```json
{ "processorModel": "haiku" }
```

The model an isolated processor runs on. A processor rewrites or reads a file nobody vouched for, so
a large rewrite is billed for the whole file in and the whole file out. Naming a cheaper model here
moves those calls onto it, while the planner keeps the model the session is using. A
[manifest run](../using/manifest-mode.md) plans on the run's model and carries
out each processing step on this one. Without the key, and with `"inherit"`, a processor runs on the
session's model. [`--processor-model`](../reference/cli.md#--processor-model-name) names the model for
one run and wins over the key.

A processor still has no tools and no memory, and a write of what it produced is approved from the
diff of the real file, so a weaker model that rewrites a file worse shows up in that diff.

A model that needs a sign-in this machine has not made, that your administrator refuses, or that no
configured service answers for is reported and the call is not sent to the session's model instead,
since naming a cheaper model is a limit on what bulk work costs. A processor answered by a model
other than the one named says so.

**The key is read from `~/.bravebot/settings.json` and from the file `--settings` names, and from no
other.** The model is sent the files a processor reads, so a `.bravebot/settings.json` or
`.bravebot/settings.local.json` that names one is ignored and reported by `bravebot doctor`.

The name is read as [`model`](#model) is, so `opus`, `sonnet` and `haiku` name a tier.

### `summaryModel`

```json
{ "summaryModel": "haiku" }
```

The model that compaction summaries and `/goal` checks run on. Both are side requests rather than
the work itself, so a session on an expensive model can summarise its own history and judge its
stopping condition on a cheaper one. Without the key both run on the model the session is using.

A model that needs a sign-in this machine has not made is reported, and the request is not sent to
the session's model instead. A compaction refused this way leaves the conversation as it was, and a
goal check refused this way ends the goal. Naming a cheaper model is a limit on what background work
costs, and quietly falling back would spend exactly what the key was set to avoid.

A model no configured service answers for is reported the same way, so a typo in this key stops the
request instead of sending it somewhere that answers without your conversation.

A model your administrator refuses is not used here either. The managed settings on a machine rule a
model out for every route that could name one, and this key is a route, so it is reported the same
way rather than sent.

**The key is read from `~/.bravebot/settings.json` and from the file `--settings` names, and from no
other.** The model is sent the conversation, so a `.bravebot/settings.json` or
`.bravebot/settings.local.json` that names one is ignored and reported by `bravebot doctor`, as a
`model` is.

The name is read as [`model`](#model) is, so `opus`, `sonnet` and `haiku` name a tier. The context
budget that decides when to compact is still measured against the window of the model the session is
using, not this one.

### `planModel`

```json
{ "planModel": "opus" }
```

The model a turn runs on when it begins in plan mode. A stronger model can plan and the session's own
model can act, without switching with `/model` at each change of mode. Without the key a plan turn
runs on the model the session is using.

The mode a turn began in decides: a turn that begins in plan mode runs on this model in every
round, including over a model you picked with `/model`, and the next turn in another mode runs on
the session's model. Pressing the mode key while a turn runs does not move that turn to the other
model. A model an agent definition names for the turn, and `--model` on a one-shot run, outrank this
key, and a delegate keeps the model it would have had.

A model that needs a sign-in this machine has not made, that your administrator refuses, or that no
configured service answers for is not used: bravebot says so and the turn runs on the session's
model, as a plan turn is not worth refusing for a mistyped name. Each change of mode writes the
conversation to the model's cache again, since a cache belongs to one model.

**The key is read from `~/.bravebot/settings.json` and from the file `--settings` names, and from no
other.** The model is sent the conversation, so a `.bravebot/settings.json` or
`.bravebot/settings.local.json` that names one is ignored and reported by `bravebot doctor`, as a
`model` is.

The name is read as [`model`](#model) is, so `opus`, `sonnet` and `haiku` name a tier.

### `references`

```json
{
  "references": {
    "libfoo": "~/src/libfoo",
    "notes": { "path": "/Users/me/notes", "description": "my design notes" }
  }
}
```

Directories outside the project that you keep for the planner to consult: a library's source, a
sibling repository, a folder of notes. An entry is a path (absolute, or starting with `~`) or an
object with a `path` and an optional `description`. A name holds no `/`, whitespace, backtick or
comma.

Each usable entry is opened the way [`--add-dir`](../reference/cli.md#--add-dir-path) opens a
directory: its files can be read by absolute path, and nothing is vouched for, so a file there is
read on the footing of any file you did not vouch for. The system prompt lists each open one with
its directory and your description. Nothing inside a reference, an `AGENTS.md` included, is read
into the prompt.

Typing `@` offers each open reference's name beside the files of the project, and a `/` after one
lists that directory, so `@libfoo/src/parser.rs` names a file in `~/src/libfoo`. Sending the line
reads it as any file named with `@` is read, and your typing it is what vouches for it. A file or
directory of the project with the same name as a reference is offered instead of the reference. Only
references that are open are offered, so one closed with `/add-dir close` drops out of the list, and
`/add-dir` and `/cd` update it when they change what is open. A reference directory whose path is not
valid text cannot be named this way: the name is refused rather than read from another directory.

An entry with a bad name or no path, or one whose directory is missing, is not a directory, lies
inside the working directory or is refused by `permissions.readsStayInWorkspace`, is reported by
name and skipped, and the other entries still apply. A later file's block replaces an earlier one's.

**The key is read from `~/.bravebot/settings.json` and from a file `--settings` names that sits
outside the project.** A project or local settings file that names it is ignored and reported by
`bravebot doctor`, since it would make a directory reachable and put its description in front of the
planner without your having written either.

### `agent`

```json
{ "agent": "rule-reviewer" }
```

The name of one of your [definitions](agents.md). A session, a session in lines or a one-shot run
that is given no [`--agent`](../reference/cli.md#--agent-name) addresses every turn to it, as the
flag would. `--agent` outranks the setting, and so does the name recorded by a session you resume.
`/status` and the first line of the session say that the setting chose it. It does not apply to
`--mode manifest`.

If no definition has the name, bravebot says so and the session or run goes on without one, where
`--agent` would refuse it. A definition whose model needs a sign-in you have not made is refused.

**The key is read from `~/.bravebot/settings.json` and from the file `--settings` names, and from no
other.** A `.bravebot/settings.json` or `.bravebot/settings.local.json` that names one is ignored and
reported by `bravebot doctor`, since it would choose the prompt and the tools of every turn from a
file you did not write.

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

Everything under [Choosing how hard to think](effort.md) still applies, including
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

### `limit`

```json
{ "limit": "500k" }
```

The most tokens or Leo Premium credits a session may spend before the terminal client asks whether to
stop, to go on without a limit or to go on under a new one. A whole number of tokens, or a string with
`k` or `m` after it for thousands or millions. Put `credits` after the figure, as in `"200 credits"`,
to count the credentials a Leo Premium subscription spends, one per request, instead. A value that is
none of these, and zero, sets no limit. The nearest settings file that names it wins, and
[`/limit`](../reference/commands.md#limit-amount--off) changes it for one session. Tokens are
the usage the model service reports, so a limit in tokens is a bound on tokens and not on money. The
one-shot `-p` run and the desktop app do not apply it.

### `maxRunningDelegates`

```json
{ "maxRunningDelegates": 32 }
```

The most delegates one turn may have running at once, counted across every level of nesting. Without
this key it is 64. A delegate that has finished gives its place back, so a turn can start more than
that number over its life and never has more going at once. A request made while every place is
taken is refused and says how many the turn may have running, and a fan-out that meets the number
part-way starts the ones that fit. A whole number above zero; zero, a negative or fractional number,
or a number in quotes leaves it at 64. The nearest settings file that names it wins. Each delegate
running holds a context of its own and may have a model request in flight, so a larger number raises
the most a turn can spend at once, and a smaller one lowers how many requests reach your gateway at
the same moment. The planner and the delegates cannot change it.

### `deferMcpToolsAbove`

```json
{ "deferMcpToolsAbove": "20k" }
```

Every tool of a server you approved is otherwise sent to the model in full on every turn. With this
set, a turn whose server tools would weigh more than that many tokens, estimated from their length,
sends only their names, `alias:tool`, and a `load_tool` the model calls with one name to have that
tool's definition sent from its next request on. The name has to match one on the list exactly.
A call to a listed tool that has not been loaded is refused with the instruction to load it first, and is
neither put to you nor sent to the server.
Loading changes what is sent and not who is asked: a loaded tool still asks you before each call.
A `reader` or `checker` delegate, a delegate left with no server, and a run limited with `--tools` are
offered no `load_tool`.
A number or a size like `"20k"`; anything else, and zero, leaves every tool in full, which is the
default. It lasts a turn, so a later turn loads again.

### `retentionDays`

```json
{ "retentionDays": 14 }
```

How many days a session record, a language server index and a file of findings are kept after
anything last wrote them. When a session starts, each one older than that is removed: a record goes
with its audit trail and its search index, and an index goes once no language server has started on
it for that long. Without this key it is 30. A whole number of one or more; one day is the least, so
nothing written in the last day is removed.

Only `~/.bravebot/settings.json` states it. A project or local settings file, and a file named with
`--settings`, cannot, since a project's settings arrive with a clone and one saying `1` would remove
your records by being opened. `bravebot doctor` names a file that tried.

If your own settings file cannot be read, or `retentionDays` is zero, negative, fractional, a string
or anything else that is not a whole number of days, nothing is removed until it can be, and `doctor`
names the file. The built-in 30 is not used in its place, since a record removed on a period you did
not choose cannot be brought back.

The session you are starting is never removed, so resuming a record older than the period works, and
neither is a session running in the background or the language server index of the workspace you
start in. Your
rules, your answers to the startup question, your prompt history and the acceptances of findings are
not swept. An [incognito](../using/incognito.md) session removes
nothing. A session left open in another terminal for longer than the period has its record removed by
a new one, and its next save writes it again.

To keep a conversation past the period, run [`/export`](../reference/commands.md#export-path) first, or
state a larger number.

### `awaySummaryEnabled`

```json
{ "awaySummaryEnabled": false }
```

After the terminal has been unfocused for five minutes (or the
[`awaySummaryIdleSeconds`](#awaysummaryidleseconds) you set) since the last turn finished, a session
of at least three turns recaps itself once, as `/recap` does, and not again until another turn finishes.
It needs a terminal that reports when it is left. `false` turns the automatic recap off, and `/recap`
still works. Only the boolean `false` turns it off: `"false"` in quotes, or any other value, leaves it
on.

### `awaySummaryIdleSeconds`

```json
{ "awaySummaryIdleSeconds": 120 }
```

How many seconds the terminal has to have been unfocused since the last turn finished before
[`awaySummaryEnabled`](#awaysummaryenabled)'s automatic recap is made. Without this key it is 300.
A whole number above zero; zero, a negative or fractional number, or a number in quotes leaves it at
300. It changes nothing else about the recap, and `/recap` is not delayed by it.

### `askIdleSeconds`

```json
{ "askIdleSeconds": 600 }
```

How many seconds a question from the agent, put with [`ask_user`](../reference/tools.md#ask_user),
waits with no key pressed before the terminal closes it. Without this key it waits for as long as it
takes. The question on screen and every question behind it are closed without an answer, the
answers already given are kept, and the agent is told that no answer was given in the time allowed
and that you may be away, so it can go on with its own judgment and ask again later. Any key press
starts the time again. What you had marked or half typed on the question that closed is not sent.
The session says that a question was closed, and a question that closed is asked again if the agent
asks it again.

A whole number above zero; zero, a negative or fractional number, or a number in quotes leaves a
question waiting. Only your own file and a `--settings` file outside the project set it: a
`.bravebot/settings.json` that arrived with a checkout is ignored and named by `bravebot doctor`,
since a question closed on a clock is one you were not asked.

It closes the agent's questions in the terminal interface only. A plan to approve, a spend limit and
a repeated call always wait for you, and so do questions in a session started with `--plain`, in a
background session, and in the desktop app.

### `notificationMethod` and `notificationCondition`

```json
{ "notificationMethod": "osc9", "notificationCondition": "unfocused" }
```

When a turn ends, or an approval or a question from the agent opens, the terminal
interface writes one notice to the terminal, so you learn it without looking at the window. The text
is fixed (`bravebot: turn finished` or `bravebot: waiting for an answer`) and never carries a command,
a path or any part of a reply.

`notificationMethod` is `osc9` (a desktop notification in terminals that read OSC 9), `bel` (the terminal
bell), `off`, or `auto`. `auto` is the default, and so is any other value: it writes OSC 9 where
`TERM_PROGRAM` is iTerm.app, WezTerm or ghostty, and the bell elsewhere. Inside tmux or over ssh
`TERM_PROGRAM` often does not name your terminal, so `auto` writes the bell there; set `osc9` to ask for
the notification instead.

`notificationCondition` is `unfocused` or `always`. `unfocused` is the default, and so is any other
value: the notice is written only after the terminal reported being left, so a terminal that reports no
focus changes is never written to. `always` writes it whether or not the terminal is in front.

### `suggestions`

```json
{ "suggestions": true }
```

Off by default. With `true`, once a turn has ended with a reply, the model `summaryModel` names (the
session's own where it names none) is asked for the next prompt you are likely to type, and the
box shows it dimmed where the invitation was. Tab or Right moves it into the box as text you can
edit; nothing sends it, and a suggestion that begins with `/` is only text until you press Enter.
Any other key removes it. Each one is a request that costs tokens, charged to the session and shown
by `/cost`. None is asked for in plan mode, in an incognito session or after a turn that failed or
was stopped. Only the boolean `true` turns it on.

### `terminalTitle`

```json
{ "terminalTitle": false }
```

The terminal's title names the session, as `bravebot · dependency audit`
([naming a session](../using/sessions.md#naming-a-session)). `false` leaves the title alone, for a
terminal or multiplexer that manages titles itself. Only the boolean `false` turns it off: `"false"`
in quotes, or any other value, leaves it on. An incognito session leaves the title alone whatever this
says.

### `deepLinks`

```json
{ "deepLinks": false }
```

`false` makes `bravebot --handle-uri`, which the operating system runs for a `bravebot://open` link,
refuse every link with the configuration status before it reads one. Only the boolean `false` does
this: `"false"` in quotes or any other value leaves links on. It does not remove the registration the
installer made; set `BRAVEBOT_NO_DEEP_LINKS=1` when running `install.sh` to leave that out, or delete
`bravebot-handler.desktop` from `~/.local/share/applications`.

### `tui.mouse`

```json
{ "tui": { "mouse": false } }
```

`false` stops the session asking the terminal to report the mouse. The terminal's own text selection
and scrollback wheel then work as they do outside a session, and the session's own drag selection
and wheel scrolling do not (use the keys and the scroller instead). Only the boolean `false` does
this: `"false"` in quotes or any other value leaves capture on. Read when the session opens.

While capture is on, most terminals still select natively if you hold a modifier while dragging:
Shift on most, Option on macOS iTerm2 and Terminal.app.

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

Variables to withhold from a program the agent runs. See [Sandbox settings](../security/sandbox.md#runscrubenv).

### `run.network`

`open` (the default) or `closed`: whether a program the agent runs keeps the network. See [Sandbox settings](../security/sandbox.md#runnetwork).

### `sandbox.network`

The hosts a program the agent runs may reach. See [Sandbox settings](../security/sandbox.md#sandboxnetwork).

### `sandbox.filesystem`

Lists of paths that move what a program the agent runs reads and writes. See [Sandbox settings](../security/sandbox.md#sandboxfilesystem).

### `sandbox.holdBackTemporaryDirectory`

With `true`, no program the agent runs writes the system temporary directory. See [Sandbox settings](../security/sandbox.md#sandboxholdbacktemporarydirectory).

### `git.identities`

```json
{ "git": { "identities": { "github.com/your-account": "~/.ssh/your-account.pub" } } }
```

Which of your ssh keys a `git` command the agent runs offers a host first, for the repositories of
one owner. Each name is `<host>/<owner>`, where the owner is the account or organisation the
repository's path starts with, and each value is a public key file, absolute or starting `~/`; a
name without `.pub` reads the file beside it. Use it when your ssh agent holds keys of more than one
GitHub account: ssh offers the agent's keys in order and GitHub accepts the first one any account
holds, so without it every push goes out as one account.

It applies where the command's own arguments name an ssh address of that owner, such as
`git push git@github.com:your-account/repo.git main` or `ssh://git@github.com/your-account/repo`.
The command then runs ssh as `ssh -i '<key>' -o IdentitiesOnly=yes`, which offers that key first and
after it only keys your `~/.ssh/config` names, and is lent that one public file. The run prompt names
the key. A remote given by name, such as `origin`, is not matched, because its address is in the
repository's own configuration; nor is an https address. Your ssh agent still signs, so no private key
is read.

When a `git` command that carries the remote scope fails, the agent is told which key that command
offered (the one your entry names, or whichever key ssh tried first because no entry applied, or
because the command named its remote by name). It is also told that a permission rejection usually
means the key belongs to another account, and that only you choose the key. It then asks you to add
an entry and does not retry the same push.

On the commands it applies to it replaces a `GIT_SSH_COMMAND`, `GIT_SSH` or `core.sshCommand` of
yours. Your own file and a `--settings` file outside the project may write it; a project or local
file's is ignored, and `bravebot doctor` names it, an entry it cannot read, and a key it would not
lend: a path that is not a regular `*.pub` file, one inside another credential location such as
`~/.aws`, or one holding `'`, `%` or `$`. A key inside a directory the session writes is not lent
either.

Without an entry for the owner, the terminal interface asks instead. Where a `git` command such as a
push names an ssh address of an owner nothing has chosen a key for, and ssh could offer two to nine
of your public keys (the ones at ssh's default names and the ones an `IdentityFile` line in
`~/.ssh/config` names), the run prompt numbers them. Press a key's number to run the command with
that key offered first, or `0` to leave the key to ssh. Either answer is kept in
`~/.bravebot/reach.jsonl` and decides every later `git` command that reaches that owner over ssh, in
every session, as an entry would; `y` runs the command without choosing, and the next prompt asks
again. An entry in your settings decides over a kept answer. `/reach keys` lists the answers kept,
and `/reach keys remove <number>` forgets one. The plain interface and the desktop app do not ask.

When a push over ssh fails, the failure text says whether the session's ssh agent was lent to the
command, how many public key files the command could read, and how many an `IdentityFile` line named
that it could not read. It promises a numbered key prompt only where one will be shown. Where the
session accepts the request, it also says the push can be run with `'unconfined': true`, which you
approve, and which runs it with no sandbox.

After a push is refused for permission, the agent may retry with `ssh-key` in the `scopes` of its `run`
call. The run prompt then numbers the keys again for an owner you already chose for, and your new
answer replaces the earlier one. The request lends nothing, does not change an entry in your
settings, and is asked about every time and never remembered.

### `download.maxBytes`

```json
{ "download": { "maxBytes": 524288000 } }
```

The most bytes one `download_url` call may write to a file. A body over it is not saved, and the
agent is told that this setting names the limit. Without this key the figure is 100 MiB. It is a
budget on your disk, not on the conversation, because the bytes never enter the conversation. Zero
and anything that is not a whole number leave the built-in figure in force, and the nearest settings
file that names one wins.

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

An entry in `deny`, `ask` or `allow` can also be an object, which is the same rule with a note on
why it exists:

```json
{ "permissions": { "deny": [{ "rule": "Bash(git push *)", "reason": "pushes go through review" }] } }
```

The `reason` is read only from `~/.bravebot/settings.json`. In a project file, a local file or a
`--settings` file the rule still applies and the reason is ignored. A blank reason, or one that is not
text, counts as none. An object with no `rule` text is reported as an entry that could not be read.
When a `deny` rule with a reason refuses a read or a write, the refusal the planner gets quotes it
("The user's rule gives a reason: ..."), so it can pick another way instead of retrying.
[`bravebot permissions check`](../reference/cli.md#permissions-check) prints it too.

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
| `/x` | the workspace, in `.bravebot/settings.json` and `.bravebot/settings.local.json`; the directory the settings file is in, in `~/.bravebot/settings.json` and a file `--settings` names |
| `x` or `./x` | the workspace |

So a single leading slash is **not** the filesystem root. A specifier with no slash in it is a name
and matches at any depth, which makes `Read(.env)` and `Read(**/.env)` one rule; one written from
`./` stays at the top of the workspace. Relative and
absolute patterns are separate namespaces and neither reaches into the other. A `/x` rule that
starts inside the workspace is a relative one, so `Read(/secrets/**)` in a project's
`.bravebot/settings.json` is the same rule as `Read(./secrets/**)`.

Whitespace at either end of a path specifier makes the rule unreadable, since it could be padding or
part of a file's name, and `doctor` names the rule. `Read( .env )` is dropped for that reason. Write
a name that starts with whitespace from `./`, as `Read(./ notes.md)`. A name that ends with whitespace
has no spelling, since `Read(./notes.md*)` covers it along with other names. Whitespace inside a name
is fine: `Read(./my notes.md)` is about the file `my notes.md`. A `*` is always a wildcard, so no rule
is about a file whose name holds one and nothing else.

On Windows a path on a drive is a full path, with the drive as its first segment:
`Read(//D:/work/secrets/**)` covers `D:\work\secrets\key` whichever separator the path is written
with, and whichever case the drive letter is in. A rule written from the drive letter is the same
rule, though in a JSON file each backslash is doubled, `"Read(D:\\work\\secrets\\**)"`, so the
forward-slash form is easier to write. `~/x` starts at your home directory on its drive in the same
way. `Read(/D:/work/**)`, with one slash, still starts where any `/x` rule does. A workspace
rule such as `Read(.env)` does not cover a file on a drive outside the workspace.

**A one-segment relative pattern floats where it restricts and not where it grants.** `Edit(src/**)`
in `deny` or `ask` covers a `src` directory at any depth, including a copy under `vendor`; the same
pattern in `allow` covers only the `src` at the top. Anchor it as `Edit(./src/**)` to pin it to the
top of the workspace in either list.

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
machine-local one, a `--settings` file and [the one an administrator pinned](managed-settings.md)
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
as a sentence addressed to it. The block is read from your own file and the file `--settings` names
only; a project or local file that sets it is ignored and reported by `bravebot doctor`.

### `keybindings`

```json
{ "keybindings": { "stash": "alt-s", "scroller": "alt-o" } }
```

Thirteen actions can be moved and nothing else can (twelve on Windows, which has no `suspend`). A chord is spelled `ctrl-x`, `alt-o` or `ctrl+x`:

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
| `scope` | `ctrl-n` | switch Up between every stored prompt and this session's |
| `suspend` | `ctrl-z` | suspend the session to the shell ([Interactive mode](../using/interactive-mode.md#suspending-to-the-shell)) |
| `undo` | `ctrl-_` | put back the last edit to the line ([Interactive mode](../using/interactive-mode.md#undoing-an-edit)) |
| `enhance` | `ctrl-p` | rewrite the line to be more specific ([Interactive mode](../using/interactive-mode.md#enhancing-a-prompt)) |

**A chord has to carry Ctrl or Alt.** Every unmodified key is already answered (a character is typed,
Enter sends, Escape clears, Tab takes what is offered, the arrows move), so handing one to an action
would take it out of the alphabet. `shift-a` and `ctrl-shift-a` name an event a terminal never
reports, and Ctrl-C, Ctrl-D, Ctrl-J and Shift-Enter are refused because leaving and starting a line
are not rebindable.

**Every action keeps a key of its own.** A chord that cannot be read, or that the input box already
answers, leaves that action on its default. So does one two actions both asked for: both fall back
rather than one winning, since which won would come down to the order the file was read in. Two
actions *trading* chords is not a conflict and both get what they asked for. `scope`, `undo`,
`suspend` and `enhance` are the newest of the thirteen, so a file that already gave `ctrl-n`, `ctrl-_`, `ctrl-z` or `ctrl-p` to
another action now asks for a chord the new action stands on, and that action is back on its default
until the file moves `scope`, `undo`, `suspend` or `enhance` as well.

The block layers per action the way `env` does, so a project file moving one action says nothing about
the others. See [Interactive mode](../using/interactive-mode.md) for what the keys do.

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

A file an administrator writes answers above the process environment and every other source. See [Settings pinned by an administrator](managed-settings.md).

<a id="importing-from-claude-code-opencode-or-ollama"></a>

## Importing from Claude Code, opencode or a local model server

A first run with nothing configured, in a terminal, reads what Claude Code and opencode set up in your
home directory, and asks a running Ollama, LM Studio and llama-server what each serves, before it names
the three routes. Where any of them reaches a service bravebot can use, it shows what it would add to
`~/.bravebot/settings.json` and asks:

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

A second request asks which models are loaded. For the model shown, the question gives the context
window the server loaded it with. Where that is below the 131,072 tokens bravebot otherwise budgets
against, the question says the server drops the earliest turns beyond it and names
`OLLAMA_CONTEXT_LENGTH`, and the entry written carries the figure as `contextWindow`, so a
conversation is kept within it. A model that is not loaded has no line.

**A running LM Studio or llama-server.** Each is asked once, the way Ollama is, on this machine only,
with no key, and says nothing where it does not answer. A server that answers is offered as a block
of the same shape as Ollama's, under the id `lmstudio` or `llama-server`.

- LM Studio is asked for its model list at `http://127.0.0.1:1234`. It reads no variable for where it
  listens, so a server on another port is named in `BRAVEBOT_LM_STUDIO_HOST`, written as `OLLAMA_HOST`
  is. Only a language model trained for tool use is offered, and the one written is the first that
  LM Studio has loaded, or the first it lists where none is loaded. The context length it has the
  model loaded with is shown, and written as `contextWindow` where it is below 131,072 tokens.
- llama-server is asked at the port `LLAMA_ARG_PORT` names, which is the variable llama-server itself
  reads, on the address `LLAMA_ARG_HOST` names. With no port named, bravebot asks at `9931`, the port
  llama-server defaults to, and at `8080`, the one earlier releases and its container image use.
  It reports no capabilities and no loaded context, so neither is checked or shown.

```
LM Studio is running at http://127.0.0.1:1234, serving models bravebot can use.
Importing it adds these to /home/you/.bravebot/settings.json:
  model: "lmstudio/qwen/qwen3-coder-30b"
  provider.lmstudio, reached at http://127.0.0.1:1234/v1: {"name":"LM Studio (local)","options":{"baseURL":"http://127.0.0.1:1234/v1"}}
Import this from LM Studio? [y/N]
```

A server that asks for a key is not offered, because the request carries none. Where more than one
of Ollama, LM Studio and llama-server answers, each is asked about in that order, and one at a port an
earlier answer already wrote is not offered again.

**What can be imported.** A Claude Code Bedrock setup, under bravebot's own names, when a region is
named. An opencode `provider` entry reached through an OpenAI-compatible SDK, with only the fields
bravebot reads, and an `auth.json` API key for a gateway whose endpoint bravebot knows. An `openai` entry that names
no SDK, or an `auth.json` key for `openai`, imports; one naming `@ai-sdk/openai` is left as another
SDK's. The `ANTHROPIC_API_KEY` in Claude Code's settings file imports as an `anthropic` entry,
unless a Claude Code `ANTHROPIC_BASE_URL` is stated beside it. opencode's
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
used, such as an Anthropic bearer token or a Vertex AI setup, is named with the reason before the routes,
and its value is never shown.

A run that cannot ask (`-p`, `--json`, a pipe or a redirect, `doctor`) imports nothing, and its
refusal names the command that asks:

```sh
bravebot import-providers
```

That command asks the same questions at any time, whether or not a service is configured. It refuses
without a terminal and in an incognito session. An incognito session asks none of the servers either,
since it has nothing to write.

## Reaching a model through AWS Bedrock

Set `BRAVEBOT_USE_BEDROCK` and `AWS_REGION` to reach models through your own AWS account, and name
the models with the `ANTHROPIC_DEFAULT_OPUS_MODEL`, `ANTHROPIC_DEFAULT_SONNET_MODEL` and
`ANTHROPIC_DEFAULT_HAIKU_MODEL` variables or a `provider` block keyed `amazon-bedrock`. The models
are offered in `/model` beside Brave's roster. [Reaching a model through AWS Bedrock](providers/bedrock.md)
has the variables, the signing in, and the assumed context window.

## Reaching an OpenAI-compatible gateway

A `provider` block in a settings file names a gateway, the models it offers, the variable that
holds its credential and any extra headers its requests carry. It is opencode's block, so one copied
from `opencode.json` works unedited, and a local Ollama needs no key.
[Reaching an OpenAI-compatible gateway](providers/openai-compatible.md)
has the fields, the credential, the headers, which models are offered, and how to name one.

OpenAI is reached with an API key by a block keyed `openai` naming `OPENAI_API_KEY` in `env`, and
needs no `baseURL`. [OpenAI](providers/openai-compatible.md#openai) has the details, including which
models `/model` offers.

Claude is reached with an Anthropic API key by a block keyed `anthropic` naming `ANTHROPIC_API_KEY` in
`env`, and needs no `baseURL`. It is spoken to in Anthropic's own Messages protocol, not the
OpenAI-compatible one. [Anthropic](providers/anthropic.md) has the details, including the reply
ceiling and which models `/model` offers.

Google Vertex AI is reached by a block keyed `google-vertex`, or with no block by exporting
`GOOGLE_API_KEY`, with `GOOGLE_CLOUD_PROJECT` beside it for a particular project and none for express
mode. It has no model listing a key can call, so `/model`
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
