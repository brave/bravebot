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
bravebot mcp add weather --env PATH --stdio -- npx -y @dangahagan/weather-mcp@latest
```

A service somewhere else:

```sh
bravebot mcp add docs --http https://mcp.example.com/mcp
```

The alias, `weather` here, is the name you give the server: letters, digits, `-` and `_`, starting
with a letter or a digit, at most 64 characters. Giving `add` an alias that is already declared
replaces that declaration.

`--stdio` takes the program and its arguments after a bare `--`, each one as its own argument. They
are never joined into a line and never read by a shell, so an argument holding a space is one
argument, and a flag of bravebot's own among them, such as `--settings`, is the server's.

| Flag | What it does |
|---|---|
| `--env <name>` | pass this variable to the server, by name; repeatable |
| `--dir <path>` | the directory the server runs in, kept as the absolute path it resolves to |
| `--stdio -- <program> [args...]` | a program on this machine |
| `--http <url>` | a service at this url; takes neither `--env` nor `--dir` |

### A server gets only the variables you name

A server starts with an empty environment. `--env PATH` names one variable to hand it, and its value
is read from your own environment when the server starts, which is also why `PATH` has to be named
for a server whose program is found through it.

**A value is never written down.** `--env WEATHER_TOKEN=...` is refused, and so is an `env` block in
the file, and the refusal names the variable without repeating what it was set to:

```
BB1002: weather was not declared: WEATHER_TOKEN is given a value: a declaration names the variable, and its value is read from your environment
```

A url carrying a user or a password is refused for the same reason.

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

`y` approves it. Anything else, a blank line included, leaves it declared and not approved.
`bravebot mcp approve weather` asks the same question later.

The question is asked only where both standard input and standard output are a terminal. With
either piped, `add` writes the declaration and tells you to run `approve` at a terminal, and
`approve` is refused.

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

A checkout says which servers it expects in its `.bravebot/settings.json`:

```json
{ "mcp": { "request": ["weather"] } }
```

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
`--plain` it is asked after the question about trusting the directory.

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

## Each call

Every call is put to you, with the arguments the model wrote:

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

What the server returns is untrusted. You see it behind the margin; the model is handed a reference
to it and the word that it is quarantined, and none of what it says.

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

An `ask` rule asks whatever answer 2 said, and an `allow` rule answers yes for you. A rule matches
the server and the tool and never the arguments.

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
since that answer was given at the question and names one tool in one project.

`--dangerously-skip-permissions` answers both questions yes without drawing them, makes no check of
the list, and records nothing: a later run without it asks each question again.

## What a server can reach

A program server is started confined, and on a platform with no confinement for one, which is
Windows today, it is not started. It gets:

- the variables you named with `--env`, read from your environment as it starts, and no others;
- read access to the directories the `PATH` you named lists and the one its program is in, and for a
  `bin` directory the installation around it;
- the directory you gave with `--dir`, to read and write and start in, or the temporary directory
  if you gave none;
- the network, and the machine's own system directories;
- a look at any path, which says whether something is there and what kind of thing it is, and
  not what a file holds or what a directory lists.

It does not get your home directory, other than a `PATH` entry inside it such as `~/.local/bin`, and
it does not get the workspace unless `--dir` names it. A program named without a path is looked for
only in the `PATH` you named: without `--env PATH` it is not found, and the session says to name it
or give the program's full path.

A service server is reached through the same network gate as everything else the session sends, and
a redirect off the host and port you declared is refused.

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
  docs     http   unapproved  256e540f
  weather  stdio  approved    25edc5e8
```

An unapproved server is listed as unapproved rather than left out. An entry that cannot be used,
because it was edited by hand into something a declaration cannot be, is listed with what is wrong
with it, and the list then fails so that a script notices.

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
to is kept.

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
under `~/.bravebot`, so `add`, `approve` and `remove` are refused in one. On a machine that names no
profile directory there is no `~/.bravebot` at all: nothing is declared there, and nothing can be.

## Known costs

- **The full-screen interface asks before it asks about the directory.** A server you approve can
  start for a session whose directory you then decline, and runs until bravebot exits.
- **A runner cannot write its cache in your home directory.** Pass a cache variable with `--env` and
  point it into `--dir` or the temporary directory. A runner from a toolchain installed under your
  home directory, as `nvm` installs one, does not start confined.
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
- **A one-shot run still checks a list it then refuses.** The check is a model call nobody reads
  the answer to, made once per run for each server whose list you have not said yes to.
