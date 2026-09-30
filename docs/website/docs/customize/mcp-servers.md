---
sidebar_position: 7
title: MCP servers
description: Declare a Model Context Protocol server, approve it, have a checkout ask for it, and see what a session started.
---

# MCP servers

A Model Context Protocol server is a program of somebody else's, or a service somewhere else, that
offers tools. Before one can be used you declare it, which says what it is, and you approve it,
which says you have read what it will run.

## Where a server is declared

One file, in one place: `mcp.json` in `~/.bravebot`, the directory holding what is yours. It is
written by `bravebot mcp add` and read by the other `bravebot mcp` commands.

No other file declares a server. This is deliberately *not* `settings.json`, and it is not a
`.mcp.json` at the root of a checkout either. The agent writes files inside the workspace as its
job, so a file there that could name a program to run would be a way for one ordinary edit to put a
program on your machine. A [settings file](configuration.md) may name a destination and never a
command.

A settings file that tries is not quietly ignored, since whoever wrote it believes it works.
`bravebot doctor` names every `mcpServers` key, and every key under `mcp` other than `request` and
`deny`, with the file it is in, and fails:

```
  ignored   mcpServers in /work/app/.bravebot/settings.json declares an MCP server, which only ~/.bravebot/mcp.json may: nothing in it is started
```

It names the key and nothing inside it, because what is inside may be a command line and a token.

## Adding one

A program on your machine, speaking over its standard input and output:

```sh
bravebot mcp add weather -- npx -y @dangahagan/weather-mcp@latest
```

This is the line `claude mcp add` takes, and it declares the same server here.

A service somewhere else:

```sh
bravebot mcp add docs --http https://mcp.example.com/mcp
```

The alias, `weather` here, is the name you give the server: letters, digits, `-` and `_`, starting
with a letter or a digit, at most 64 characters. Giving `add` an alias that is already declared
replaces that declaration.

Everything after a bare `--` is the program and its arguments, each one as its own argument. They
are never joined into a line and never read by a shell, so an argument holding a space is one
argument, and a flag of bravebot's own among them, such as `--settings`, is the server's.

| Flag | What it does |
|---|---|
| `-e`, `--env <name>=<value>...` | store this value for the server under this name; repeatable, and one flag takes every word up to the next flag |
| `-e`, `--env <name>` | pass this variable to the server from your environment, by name; one name to each flag |
| `--dir <path>` | the directory the server runs in, kept as the absolute path it resolves to |
| `-- <program> [args...]` | a program on this machine; `--stdio --` means the same |
| `--http <url>` | a service at this url; takes neither `-e` nor `--dir` |
| `-s <scope>` | which settings file [asks for the server](#asking-for-one-from-a-checkout) once you approve it: `local`, the default, `project` or `user`; it may come before the alias |

### A server gets only the variables you give it

A server starts with an empty environment. `-e WEATHER_TOKEN=...` stores a value for it, the way a
server's own install line for Claude Code gives one, and `-e WEATHER_TOKEN` alone names a variable
whose value is read from your own environment when the server starts. A program given by name
rather than as a path, such as `npx`, is found through `PATH`, so `add` names `PATH` for it unless
you stored one, and you see it at the question below.

**A stored value is kept in `~/.bravebot/mcp.json`**, which only you can read, and never in a
settings file. It is shown by its name and never as itself, by `add`, `get`, `list` and `doctor`
alike, and one that names a file is shown as the file it may read:

```
            variables: BRAVE_API_KEY_FILE (stored), PATH
            may read: /Users/you/keys/brave-api-key
```

A value that names a file, such as brave-search's `BRAVE_API_KEY_FILE`, is one the server may read,
and so is an argument that does: `add` records that one file, shows it as `may read`, and the server
is let read it and nothing beside it. A directory, and anything in `~/.bravebot`, is never recorded.

A word `-e` cannot read is refused by its place and not repeated, since it may be a key:
`-e WEATHER_TOKEN sk-live` stores nothing, and the refusal says a name alone is read from your
environment only as the one word its `-e` takes. So is an `-e` before the alias, a `NAME=value`
typed where the alias, `-s` or `--dir` expects its word, and a name given twice. A url carrying a
user or a password is refused as well.

## Approving one

Typing `add` is not the approval. Once the declaration is written, `add` shows you what it declares
and asks:

```
declared weather in /Users/you/.bravebot/mcp.json

  weather   stdio   npx -y @dangahagan/weather-mcp@latest
            variables: PATH
            digest: 25edc5e8

  Use this MCP server? [y/N]
```

`y` approves it and [asks for it](#asking-for-one-from-a-checkout) in the file `-s` names, so a
session in this directory starts it. Anything else, a blank line included, leaves it declared, not
approved and asked for nowhere, unless that file asks for it already: then `add` says so, since the
next session there asks you again. `bravebot mcp enable weather` asks the same question later and
asks for it too; `bravebot mcp approve weather` asks it alone. Where
[an administrator refuses](#refused-by-an-administrator) the server, `add` and `enable` still ask
for it and say it will not start.

The question is asked only where both standard input and standard output are a terminal. With
either piped, `add` writes the declaration and tells you to run `enable` at a terminal, and
`approve` and `enable` are refused.

### An approval is of what you were shown

What you approve is the **digest**: a hash of the transport, the program and every argument, the
url, the variable names and the directory. The alias is not part of it. It is written, one per
line with the alias you approved it as beside it, to `~/.bravebot/mcp-approved`. The alias is kept
so a session can say a declaration changed since you approved it; it approves nothing.

So changing anything a server would run is a server nobody approved. Pinning the version above:

```
  weather   stdio   npx -y @dangahagan/weather-mcp@1.4.0
            variables: PATH
            digest: a3042003
            changed: argv

  Use this MCP server? [y/N]
```

The old approval is gone whatever you answer, since nothing declares what it approved any more.

## Asking for one from a checkout

A settings file says which servers a session starts, as a list under `mcp.request`, and
`bravebot mcp enable` writes it:

```sh
bravebot mcp enable weather             # .bravebot/settings.local.json here, yours alone
bravebot mcp enable weather -s project  # .bravebot/settings.json here, shared with the checkout
bravebot mcp enable weather -s user     # ~/.bravebot/settings.json, every session
```

Each asks the approval question where you have not approved the server, and on a yes adds it to
that file, creating the file where it is missing and leaving the rest of it as it was:

```json
{ "mcp": { "request": ["weather"] } }
```

`bravebot mcp disable weather` takes it out of each of the three files that asks for it, and says
which; with `-s`, out of that one. The declaration and its approval stay, so `enable` puts it back
without asking. A file that does not parse, or whose `mcp.request` is not a list, is left as it
was, and so is a link in the checkout: where `.bravebot` or the file in it is a link, bravebot
does not write through it. `~/.bravebot` itself may be a link, and is written through.

`settings.local.json` is yours alone by name only: add it to the checkout's `.gitignore` to keep it
out of a commit. On macOS and Linux a file these commands write is readable by you alone, as the
files under `~/.bravebot` are.

That is a list of aliases and nothing more. Each one is looked up among the servers *you* declared.
One you have not declared is reported as the session opens, and nothing is fetched, installed or
run for it:

```
.bravebot/settings.json requests the MCP server docs, which is not declared, so nothing was installed or run for it: bravebot mcp add declares one
```

A declared server no checkout asks for is not started, approved or not.

### The question a session asks

Where a requested server is declared and nothing you answered covers it, the session asks as it
opens, before anything of the server runs:

```
  weather   stdio   npx -y @dangahagan/weather-mcp@latest
            requested by .bravebot/settings.json
            runs /opt/homebrew/bin/npx
            variables: PATH
            digest: 25edc5e8
            npx fetches what it runs when it starts
            @dangahagan/weather-mcp@latest names no exact version, so it runs whatever is published under it

  Use this MCP server?
  1. Yes
  2. Yes, and use all future MCP servers in this project
  3. No, continue without this server
  [1/2/3]
```

| Answer | What it records |
|---|---|
| 1 | the approval of this digest |
| 2 | the approval, and this project's path in `~/.bravebot/mcp-projects`, so a later server a checkout here asks for starts without the question |
| 3 | nothing; the server is not used in this session and the session goes on |

Anything else you type, and the end of the input, is 3. Answer 2 answers this question and no
other: it does not reach a server whose declaration changed since you approved it, and it does not
reach another project.

`runs` is where a program named through `PATH` was found, and is left out where you gave the path
yourself. The last two lines appear for a runner that fetches a package as it starts, `npx`,
`bunx`, `npm exec`, `pnpm dlx`, `yarn dlx`, `uvx`, `uv tool run` and `pipx run`, and for a package
that names no exact version: what you approve is the command line, and what that command runs is
decided when it runs. For `npx`, `@1.2` is not exact, since npm reads it as any `1.2.x`. Where a
flag bravebot does not know comes before the package, the line says which package runs is not
known, since that flag may take the next word as its value.

In the full-screen interface the question is asked on the terminal before the interface opens. In
`--plain` it is asked after the question about trusting the directory, or, where you told bravebot
to [remember that answer](../security/trust.md#remembering-the-answer), after the line saying so.

### Where nobody can be asked

A [one-shot run](../using/headless.md), `bravebot "..."`, asks nobody. A requested server nothing approved is left out, and
the reason is printed to stderr:

```
weather was not started: a one-shot run asks nobody, so run bravebot mcp approve weather at a terminal
```

A session with no terminal says the same. An approved server starts in either. An incognito session
asks, and a yes there starts the server for that session and records nothing.

`--dangerously-skip-permissions` answers the question yes without drawing it, and records nothing,
so a later run without it asks.

## Offering its tools to the model

A started server's tools are not offered to the model until you have read them. At the start of the
first turn you ask for, the session puts each server's list to you, whole:

```
╭ offer these tools to the model? ─────────────────────────────────────────────────────╮
│  weather offers 6 tools                                                               │
│                                                                                       │
│  the check found no attempt to give instructions in this                              │
│┃ The list names weather tools and says what each returns; nothing in it addresses the │
│┃ reader.                                                                              │
│                                                                                       │
│  The model will read each tool's name, its arguments and what the server says about   │
│  it, as shown here. Every call is still put to you. Say no if a description gives     │
│  instructions.                                                                        │
│                                                                                       │
│  weather:check_service_status                                                         │
│┃ Check whether the upstream weather APIs (NOAA, Open-Meteo) are reachable. Call this  │
│┃ after any weather tool returns an error, or before a batch of requests. Returns      │
│┃ per-service status and links to the official status pages.                           │
│                                                                                       │
│  weather:get_alerts                                                                   │
│    active_only (boolean), city_name (string), detail (string, summary | standard |    │
│    full), latitude (number), location_name (string), longitude (number)               │
│┃ Get active weather alerts, watches, warnings, and advisories for a location.         │
│  1 Yes, offer them    2 No, continue without them    ctrl-c stop the turn  ↑↓ 51 more │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

Each tool is `alias:tool`, then its arguments, then every row of what the server says about it
behind the `┃` margin, none of it cut: a yes puts exactly this text in front of the model, and
the margin marks the words that are the server's. A [check](../security/vetting.md) reads the list
first, as it reads anything else about to be believed, and its verdict is drawn above it. `↑↓`
scrolls a list longer than the screen.

| Answer | What happens |
|---|---|
| 1 | the tools are offered, and a digest of the list is written as a `tools` line in `~/.bravebot/mcp-approved` beside the server's, so a later session whose server sends the same list asks nothing |
| 2 | none of its tools is offered in this session, nothing is recorded, and the next session asks again |

A list that is not the one you said yes to says so, and asks again. Answer 2 at the server
question answers for the server and not for its list.

The model sees each tool under a name like `mcp__weather__get_current_conditions`, with a sentence
bravebot writes, saying which server it belongs to and that every call is put to you, before the
server's own words. No built-in tool can be shadowed by one, since no built-in's name starts with
`mcp__`.

A [delegate](../reference/tools.md#spawn_agent) is offered the same tools where it is a `worker`
whose [definition](agents.md) names no tools, and is put no list of its own. A definition may name
the servers its `worker` calls with an `mcpServers` line, and its delegate is then offered those
servers' tools alone. A `reader` or a `checker` is offered none of them, and nor is a delegate of a
turn that holds none. A turn you address to a definition with
[`/agent`](../reference/commands.md#agent-name-task) asks you about the lists of only the servers it
calls.

## Each call

Every call is put to you, a delegate's as well, with the arguments the model wrote:

```
  weather:get_current_conditions    (MCP)

  city_name: "Toronto, Canada"

┃ Get the most recent weather observation for a location. Use this for current weather
┃ or when asking about "today's weather", "right now", or recent conditions without a
  (e to expand)

  Proceed?
  1. Yes
  2. Yes, and stop asking for weather:get_current_conditions in this project
  3. No
```

| Answer | What it records |
|---|---|
| 1, or `y` | nothing; this call runs |
| 2 | the server, the tool and this project in `~/.bravebot/mcp-tools`, so later calls to this tool in this project run without the question |
| 3, `n` or Esc | nothing; the call is not sent, and the model is told you declined |

Enter answers nothing, so a key pressed for the last question does not answer this one. `e` shows
the whole description. Answer 2 names this one tool and none of the server's others, and never
what it was called with. Where nothing can be written, as in an incognito session, it is drawn as
not offered.

What the server returns is untrusted and private. You see it behind the margin; the model is handed
a reference to it and the word that it is quarantined, and none of what it says. It is private
because what a server read may be your own mail or calendar, so a command fed that reference is put
to you whatever you [vouched for](../security/permissions.md#vouching-for-a-command), as one fed a
file is. The words a server describes its tools with stay public, since it hands the same list to
whoever connects.

Once you let something private out of quarantine for the model, such as a server's answer after a
[check](../security/vetting.md), what the model writes from then on is private too, since it may
repeat it. Every later call to a server is then put to you with its arguments, whatever answer 2 or
a rule says, and a resumed session keeps asking. Undoing the turn that let it out stops it. A file
the model reads in your project does not start it. A session saved by an earlier version is read as
having let something private out, so every call in it asks.

### When a call fails

A tool that ran and reported its own failure is handled like any other result: you see what the
server said about it behind the margin, and the model is handed a reference to it and told the
call failed. The model is never sent the server's account of the failure.

Every other failure is reported in bravebot's own words, which name the tool, the protocol method
and the code the server answered with, and nothing the server wrote. A server that fails for a
reason only its own sentence gives, or a reply bravebot cannot parse, therefore shows a code and a
method and nothing more. A server's own error output is shown in `--plain` and a one-shot run and
not in the full-screen interface, so to read it run the server by hand.

### Asking again

```sh
bravebot mcp forget [path]
```

Drops every standing answer recorded for a project, the directory you run it in by default: each
tool answer 2 stopped asking about, and answer 2 at the server question. It says what it dropped:

```
weather:get_current_conditions is asked about again before each call in /Users/you/work/app
```

and `nothing was recorded for /Users/you/work/app` where there was nothing. A path that no longer
exists is taken as typed, so a deleted checkout can still be forgotten.

### Rules

A [permission rule](configuration.md#permissions) in the `Mcp` family decides a call before it is
put to you. `Mcp(weather)` covers every tool of that server and `Mcp(weather:get_alerts)` one tool.
`Mcp(weather:*)` is `Mcp(weather)`, and a name matches whole, so `Mcp(weather)` does not cover
`weather2`.

```json
{ "permissions": { "deny": ["Mcp(weather:get_alerts)"] } }
```

A `deny` rule refuses the call before anybody is asked, and the model is told not to retry it:

```
⏺ MCP(weather:get_alerts)
  ⎿ refused: a deny rule in the settings file covers weather:get_alerts. Do not retry
weather:get_alerts and do not look for another route to what it does: say in your reply what you
needed from it.
```

An `ask` rule asks whatever answer 2 said, and an `allow` rule answers yes for you, except after
something private was [let out of quarantine](#each-call). A rule matches the server and the tool
and never the arguments.

### In `--plain`

Both questions are a line answered yes or no, `offer these tools to the model? [y/N]` and
`Proceed? [y/N]`, after the same text with the same margin. A yes to a call is answer 1, so answer
2 is given in the full-screen interface.

### Where nobody can be asked

A one-shot run asks neither question. A list nobody said yes to is not offered, and the reason is
printed:

```
note: weather offers no tool in this session: its list was not approved
```

A call that would be put to you is refused, and an `allow` rule decides nothing there, as it decides
nothing for any tool in a one-shot run. A tool you answered 2 for in this project is still called,
since that answer was given at the question and names one tool in one project, unless something
private was [let out of quarantine](#each-call) earlier in the session, when the call is refused.

`--dangerously-skip-permissions` answers both questions yes without drawing them, makes no check of
the list, and records nothing: a later run without it asks each question again.

## What a server can reach

A program server is started confined, and on a platform with no confinement for one, which is
Windows today, it is not started. It gets:

- the values you stored with `-e NAME=value`, and the variables you named with `-e NAME`, read from
  your environment as it starts, and no others;
- read access to each file `add` showed as one it `may read`, and nothing beside it;
- a home directory of its own, which `HOME` names unless you named `HOME` yourself, to read and
  write: `~/.bravebot/mcp-home/<digest>`, one for each declaration, where a runner such as `npx`
  keeps its cache between sessions;
- read access to the directories the `PATH` you named lists and the one its program is in, and for a
  `bin` directory the installation around it, including one deep in your home directory as `nvm`
  installs one;
- the directory you gave with `--dir`, to read and write and start in, or the temporary directory
  if you gave none;
- the network by IP address, and the machine's own system directories. A local socket, such as
  the one the Docker daemon listens on, is reached only in a directory the server may write, so a
  server that runs `docker` does not reach the daemon. On Linux that needs a kernel with Landlock
  ABI version 9, and an older one leaves every socket reachable;
- a look at any path, which says whether something is there and what kind of thing it is, and
  not what a file holds or what a directory lists.

It does not get your home directory, other than a `PATH` entry inside it such as `~/.local/bin` and
the installation its program came from, and it does not get the workspace unless `--dir` names it. A program named without a path is looked for
only in the `PATH` you gave: without `-e PATH` it is not found, and the session says to name it
or give the program's full path.

A service server is reached through the same network gate as everything else the session sends. A
redirect off the host and port you declared is not followed. Nothing is sent there, and you are
asked whether the server moved:

```
  docs is declared at https://docs.example.com/mcp
  and its reply points to https://mcp.example.net/mcp
  reaching mcp.example.net
  Nothing was sent there. A yes declares the server at that address and sends it what was being
  sent, and every later request to the server goes there too, in this session and the next. Say
  no unless you know the server moved.

  declare this server where its reply points? [y/N]
```

A redirect met during a turn is asked about there. One met as the session starts the server is
asked at the terminal before the session opens. A yes makes the handshake at the new address and
only then rewrites the entry in `~/.bravebot/mcp.json` and approves it. Its list of tools is put to
you again when a session next starts it, since the list you said yes to was read at the old
address. A yes does not move it to an address a declaration cannot hold, or to a host your
organization's settings refuse. A no changes nothing: the call is refused, or the server is not
started. A one-shot run, a session with nobody at the terminal, and
`--dangerously-skip-permissions` refuse the redirect without asking.

## Connecting Gmail and Google Calendar

Reading Gmail through Google's Workspace server, with sign-in done once outside bravebot and the
tools that send mail refused, has a page of its own: [Connecting Gmail](mcp/gmail.md). Reading
Google Calendar through the same server, with the tools that change a calendar refused, is
[Connecting Google Calendar](mcp/calendar.md). Further pages for particular servers will sit beside
them if they read naturally.

## Seeing what a session started

`/status` in the full-screen interface names the servers the session started, and what became of
each list:

```
  Confinement   kernel-enforced
                  this session confines the MCP servers it started, and nothing else it runs
  MCP servers   weather
                  weather: 6 tools offered to the model
```

Before the first turn a list reads `its tools are put to you before the next turn plans`, and a list
you said no to reads `no tool offered, as you answered`. It says `none` where it started none. A requested server that was not started is not on the line;
why is said once, as the session opens.

## Seeing what is declared

```
$ bravebot mcp list
declared in /Users/you/.bravebot/mcp.json
for a session started in /Users/you/work/app
  docs     http   unapproved  256e540f
             requested by /Users/you/work/app/.bravebot/settings.json: a session at a terminal here asks before starting it, and holds a grant to call it only after a yes
             nothing is answered for it here
  weather  stdio  approved    25edc5e8
             requested by /Users/you/work/app/.bravebot/settings.json: a session here starts it unasked, and holds a grant to call it
             answered here: call get_forecast without asking
  maps     http   approved    9c1e07b2
             not requested here, so no session here holds a grant to call it
             nothing is answered for it here
```

An unapproved server is listed as unapproved rather than left out. Under each server are the file
in this directory that asks for it, whether a session started here would start it unasked, ask you
first, or not start it (with the reason), and the standing answers recorded for it in this project:
answer 2 at the server question and the tools answer 2 stopped asking about. The rows are for a
session at a terminal. Where nobody can be asked, a server that would be asked about is not started.
A server a file here asks for and nobody declared is a row of its own, ending with the command that
declares it. An entry that cannot be used, because it was edited by hand into something a
declaration cannot be, is listed with what is wrong with it, and the list then fails so that a
script notices.

`bravebot doctor` prints the same rows and fails where `list` does.

`bravebot mcp get weather` shows one declaration in full, with the whole digest:

```
declared in /Users/you/.bravebot/mcp.json
  weather   stdio   npx -y @dangahagan/weather-mcp@latest
            variables: PATH
            digest: 25edc5e806956f3254b5ff2b116f1c07ae5ad8d4a5df00f6d15c0f8aadfec43f
            approved
```

## Removing one

```sh
bravebot mcp remove docs
```

Removes the declaration and its approval together. An approval another declaration still resolves
to is kept. A settings file that asks for it still does, and is reported as asking for a server
nobody declared: `bravebot mcp disable docs` takes that out.

## Refused by an administrator

The machine's [administrator file](configuration.md#pinned-by-an-administrator) can keep a server
from starting in every session on the machine, and cannot give one. It names servers by the host a
url reaches or the command a program runs, never by the name you gave it:

```json
{
  "mcp": {
    "allow": [
      { "host": "*.example.com" },
      { "command": ["/opt/homebrew/bin/npx", "-y", "@dangahagan/weather-mcp@1.4.0"] }
    ],
    "deny": [{ "host": "staging.example.com" }]
  }
}
```

- A `host` entry matches a url naming that host, on any port and path. `*.example.com` matches
  `mcp.example.com` and `a.b.example.com`, and neither `example.com` nor `badexample.com`.
- A `command` entry matches a command line word for word, with the program as the absolute path it
  was found at, the path `mcp get` shows as `runs`. `npx -y @dangahagan/weather-mcp@latest` is not
  the command above, so it is not allowed.
- With no `allow` list, every server not denied starts. With one, only what it names starts, and
  `"allow": []` starts nothing.
- A `deny` entry wins over an `allow` entry.

A refused server is not started in any mode, including `--dangerously-skip-permissions`, nothing
is asked about it and nothing is recorded, and the session says why as it opens:

```
weather was not started, whatever was declared or approved: /Library/Application Support/bravebot/managed.json, which this machine's administrator manages, allows only the servers its mcp.allow names, and not this one
```

`list` and `get` keep showing your declaration and your approval, and add the same reason:

```
$ bravebot mcp list
declared in /Users/you/.bravebot/mcp.json
  docs     http   unapproved  256e540f
  weather  stdio  approved    25edc5e8  not started: /Library/Application Support/bravebot/managed.json, which this machine's administrator manages, allows only the servers its mcp.allow names, and not this one
```

Your approval is left where it is, so a server comes back as you answered for it once the file stops
refusing it. Only a list counts, and an entry in neither form is skipped: a deny list of nothing
else denies nothing, and an allow list of nothing else starts nothing. A url whose host is spelled
in a way bravebot and the connection could read apart, with a backslash or a percent escape for
one, matches no `allow` entry, and is refused wherever `deny` names a host. The file cannot declare
a server, approve one or answer any of the three questions for you, so a declaration or an
`mcp.request` written there is read as nothing, and `bravebot doctor` names `mcp.allow` and
`mcp.deny` among what the file pins.

A `deny` entry names one spelling. A link or a copy of a denied program, or another name for a
denied host, is not denied, and the variables a server starts with are not part of a match. An
`allow` list holds against all of those. The same keys in your own `settings.json` refuse nothing;
to be rid of a server yourself, [remove it](#removing-one).

## Where nothing is written

An [incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind) writes nothing
under `~/.bravebot`, so `add`, `approve`, `enable`, `disable` and `remove` are refused in one. A server started in one is
given a home directory in the temporary directory instead, removed once it stops, so a runner
fetches its package again each session. On a machine that names no
profile directory there is no `~/.bravebot` at all: nothing is declared there, and nothing can be.

## Known costs

- **A settings file `bravebot mcp` writes comes back with its keys sorted.** Every value is kept,
  and the spacing and order are not.
- **The full-screen interface asks before it asks about the directory.** A server you approve can
  start for a session whose directory you then decline, and runs until bravebot exits.
- **Nothing removes a server's old home.** Changing a declaration leaves the directory the one
  before it wrote under `~/.bravebot/mcp-home` until you remove it.
- **A toolchain that loads from elsewhere in your home directory does not start.** A version
  manager's shim that hands over to a program somewhere else in your home is not given that
  somewhere.
- **The full-screen interface does not show a server's own error output.** `--plain` and a one-shot
  run pass it through to stderr.
- **No local server starts on Windows yet.** There is no confinement for one there, so the session
  says so and goes on without it.
- **The desktop application starts no server yet.** Only the terminal client acts on a checkout's
  request.
- **A checkout cannot bring its own server.** A project that needs one says so in its README, and
  each person declares it. That is the point, and it costs a step per machine.
- **An approval does not travel.** It lives in your own directory, so a second machine asks again.
  The same is true of a list you said yes to and a tool you stopped the asking for.
- **`--plain` cannot stop asking for a tool.** Its call question has room for one answer.
- **What the model has read can leave in a URL or a command.** Once you let a result out of
  quarantine, every call to a server is put to you, but a fetch to a host an `allow` rule covers is
  not, and nor is a command a rule allows: a URL and a command line are not held back as private.
- **A server that holds nothing of yours still asks.** Every result is private, so a search
  server's answer fed to a command is put to you as a mailbox's would be.
- **A one-shot run still checks a list it then refuses.** The check is a model call nobody reads
  the answer to, made once per run for each server whose list you have not said yes to.
