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
| `--dir <path>` | the directory the server runs in and may write, kept as the absolute path it resolves to; one inside a git repository is refused |
| `--startup-timeout <seconds>` | how long each of the server's two handshake requests may take, from 1 to 3600; 60 where it is not given |
| `--tool-timeout <seconds>` | how long a call may take, from 1 to 3600; 120 where it is not given. A call that takes longer fails and the planner is told it timed out; a local server is then stopped |
| `-- <program> [args...]` | a program on this machine; `--stdio --` means the same |
| `--http <url>` | a service at this url; takes neither `-e` nor `--dir` |
| `--bearer-env <name>` | with `--http`: send `Authorization: Bearer` followed by the value of this variable in your environment |
| `--header-env <header>=<name>...` | with `--http`: send this header with the value of that variable in your environment; repeatable |
| `--allowed-hosts <list>` | with a program: the only hosts it may reach, host names or `*.` domains separated by commas; see [Limiting the hosts a program reaches](#limiting-the-hosts-a-program-reaches) |
| `--tools <list>` | offer only these tools of the server; names separated by commas, or by spaces up to the next flag. `--tools=a,b` also works |
| `--exclude-tools <list>` | never offer these tools, written the same way |
| `--ask-every-call` | put every call to this server to you, whatever a rule, an earlier answer or `--dangerously-skip-permissions` says, and keep no answer to one; see [Servers that ask every call](#servers-that-ask-every-call) |
| `--choice <tool>:<argument>` | with `--ask-every-call` and a program: name an argument of one of the server's tools that a choice may fill; repeatable, one pair to each flag; see [Arguments that take a choice](#arguments-that-take-a-choice) |
| `-s <scope>` | which settings file [asks for the server](#asking-for-one-from-a-checkout) once you approve it: `local`, the default, `project` or `user`; it may come before the alias |

### Choosing which tools are offered

`--tools` and `--exclude-tools` work for a program and for a service. A name is a bare word such as
`get_forecast`, which matches that tool on this server, or `alias:tool` such as
`weather:get_forecast`. A name that is not a tool name is refused, and so is an empty list.

```sh
bravebot mcp add weather --exclude-tools check_service_status -- npx -y @dangahagan/weather-mcp@latest
```

A tool on the `--exclude-tools` list is withheld, and so is a tool missing from a non-empty
`--tools` list. When both name a tool, the exclusion wins. A withheld tool is not offered to the
model and cannot be called, and the model is told there is no such tool. In `mcp.json` the lists are
the keys `tools` and `exclude_tools`; a value that is not a list of tool names is refused.

### Limiting the hosts a program reaches

`--allowed-hosts` gives a program its own list of hosts, such as `api.example.com,*.cdn.example.com`,
where `--allowed-hosts=a,b` also works. The server is held to the proxy that applies the list, as
`sandbox.network.allowedHosts` holds it, and reaches no other host. An entry that is not a host name
or a `*.` domain is refused, and so is a list with no entry. A service declared with `--http` takes
no list. In `mcp.json` the key is `allowed_hosts`.

```sh
bravebot mcp add weather --allowed-hosts api.weather.example -- npx -y @dangahagan/weather-mcp@latest
```

The list can narrow what the session allows and never widen it. Where `sandbox.network.allowedHosts`
is set, an entry of the server's that the session's list does not cover is dropped. A `*.` wildcard
is covered only by a wildcard on the same domain or on a domain above it. The session's denied hosts
still apply. The list is part of what your approval binds to, so adding, changing or emptying it asks
again, and the question shows it as `may reach only:` followed by the hosts, or `may reach no host`
for an empty list in `mcp.json`. A platform that cannot hold a program to the proxy does not start
the server, and the note names `allowed_hosts`.

### Headers for a service

A hosted service often wants a token. `--bearer-env GITHUB_PERSONAL_ACCESS_TOKEN` sends
`Authorization: Bearer` and the value of that variable. `--header-env X-Api-Key=SERVICE_KEY` sends
the header `X-Api-Key` with the value of `SERVICE_KEY`. Both read the variable from your own
environment each time the server starts, so the value is never written to `mcp.json` or to a
settings file, and no question, `get` or `list` shows it. They show the header and variable names
instead, as `Authorization: Bearer $GITHUB_PERSONAL_ACCESS_TOKEN`.

The headers go only to requests for the scheme, host and port you declared. A redirect to any other
socket drops them, and is put to you as [a server that moved](#what-a-server-can-reach). Both flags
are refused on a program server, and so is a value written out in the command.

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
user or a password is refused as well, and so is one with a `:` after the host that is not followed by a
port, which is a number up to 65535.

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

A declaration with tool lists also shows `tools: a, b` and `exclude tools: c`.

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

## Starting or stopping one during a session

A server that failed to start is left out until the session ends. In the full-screen interface,
`/mcp` lists the servers the session runs and those it asked for that are not running. `/mcp
reconnect` starts every one that is not running, and `/mcp reconnect weather` starts that one. A
start reads the declaration again and asks what a session asks as it opens: a server you approved
starts without a question, and one whose declaration changed is put to you again. `/mcp disable
weather` stops the server and drops its grant for this session. Nothing is written, so the next
session starts it as before. `/mcp remove weather` is the permanent version: it runs `bravebot mcp
disable weather` and then `bravebot mcp remove weather` for you, so no later session starts the
server or asks about it, and stops it in this session. The list `/mcp` prints, which is also what a
click on a server in the info panel prints, ends with a line naming it. Typed during a turn, the
command waits for the turn to end.

The info panel lists a server only while its tools are offered to the model. If you answer no when
you are asked whether to offer a server's tools, the server keeps running for the session, is left
out of the panel, and is still named by `/mcp` and `/status`.

### An approval is of what you were shown

What you approve is the **digest**: a hash of the transport, the program and every argument, the
url, the variable names, the header and header-variable names, the tool lists and the directory.
A declaration with no tool lists digests as it did before. The alias is not part of it. It is written, one per
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
`--plain` it is asked after the question about trusting the directory, or, where an earlier yes was
[kept](../security/trust.md#remembering-the-answer), after the line saying so.

In `--plain` the question has one answer: `y` starts the server for that session, and any other line,
including `1` and `2`, and the end of the input decline. It offers no answer that records, and what is
answered there is not written to the state directory.

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
first turn you ask for, or of the first plan run in [manifest mode](../using/manifest-mode.md) where
that comes first, the session puts each server's list to you, whole:

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

Where the server's allow or deny list withheld tools, the heading says so: "one tool was withheld by
the server's allow or deny list", or "N tools were withheld by the server's allow or deny list".

Each tool is `alias:tool`, then its arguments, then every row of what the server says about it
behind the `┃` margin, none of it cut: a yes puts exactly this text in front of the model, and
the margin marks the words that are the server's. A [check](../security/vetting.md) reads the list
first, as it reads anything else about to be believed, and its verdict is drawn above it. With
several servers the checks of their lists are made at the same time, no more than eight at once,
so the wait before the first question is about one check's. The questions come one at a time in the
order the servers are declared, and the time you take over them is counted as time spent waiting on
you, which the checks are not. When the session spends a Premium subscription, the checks are made
one after another instead. `↑↓` scrolls a list longer than the screen.

| Answer | What happens |
|---|---|
| 1 | the tools are offered in this session, nothing is recorded, and the next session asks again |
| 2 | none of its tools is offered in this session, nothing is recorded, and the next session asks again |

Answer 2 at the server question answers for the server and not for its list.

A local server that says in its handshake that it announces changes to its tools is asked for its
list again when it does. At the next turn you ask for, bravebot says the server changed its tools
and puts the list it gives now to you as above. None of that server's tools is offered until you say
yes to it, including the ones you had said yes to, and a no you gave earlier does not stand for the
new list. A remote server is not read this way.

The model sees each tool under a name like `mcp__weather__get_current_conditions`, with a sentence
bravebot writes, saying which server it belongs to and that every call is put to you, before the
server's own words. No built-in tool can be shadowed by one, since no built-in's name starts with
`mcp__`.

A [delegate](../reference/tools.md#spawn_agent) is offered the same tools where it is a `worker`
whose [definition](agents.md) names no tools, and is put no list of its own. A definition may name
the servers its `worker` calls with an `mcpServers` line, and its delegate is then offered those
servers' tools alone. The call starting a `worker` can narrow that again with
[`mcp_servers`](../reference/tools.md#spawn_agent), and `[]` starts one offered none of them. A
`reader` or a `checker` is offered none of them, and nor is a delegate of a
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
a reference to it and the word that it is quarantined, and none of what it says. A call can ask for the
reply in the same result with `read: true`, where permissions are bypassed with no screening or an
auto-vetting check says it is safe. Anything else keeps the reference. A tool that lists an argument
named `read` of its own keeps it, and `read` is not offered at all to a session without `vet_content`.
See [Asking for the result in the same call](../security/vetting.md#asking-for-the-result-in-the-same-call).
It is private
because what a server read may be your own mail or calendar, so a command fed that reference is put
to you whatever you [vouched for](../security/permissions.md#vouching-for-a-command), as one fed a
file is. The words a server describes its tools with stay public, since it hands the same list to
whoever connects.

A picture in a result, such as a screenshot, is held the same way. A part of the result is taken as a
picture only if it is a PNG, JPEG, GIF or WebP and decodes to at most 8 MiB, and every other part, such
as audio or a PDF, is left out and counted. The model is told how many parts were left out, and is
handed a reference for the text and one for each picture, naming its type and size. You are shown a
row that says what the picture is and how big, and a row saying how to look at it, behind the margin.
See [A picture that arrives](../security/vetting.md#a-picture-that-arrives).

Once you let something private out of quarantine for the model, such as a server's answer after a
[check](../security/vetting.md), what the model writes from then on is private too, since it may
repeat it. Every later call to a server is then put to you with its arguments, whatever answer 2 or
a rule says, and a resumed session keeps asking. Undoing the turn that let it out stops it. A file
the model reads in your project does not start it. A session saved by an earlier version is read as
having let something private out, so every call in it asks.

A call asked for that reason is drawn without answer 2, which is shown as not offered with the
reason that no stored answer covers a call whose arguments are private. Typing 2 anyway answers yes
to that one call and records nothing, so the next call asks again.

When the model makes several calls to the same tool in one round, you are asked once. The question
for the first call lists the arguments of each later call to that tool under a line saying your
answer also covers them. Yes makes all of them and no refuses all of them. A call to a server that
asks every call, and a call that names a choice, are asked on their own and never listed.

### Servers that ask every call

A server added with `--ask-every-call` is asked about at every call, for tools whose arguments you
want to read each time:

```sh
bravebot mcp add cookies --ask-every-call -- /path/to/program mcp
```

The question for its calls is the one above without answer 2, which is drawn as not offered, and
`--plain` says no answer to the call is remembered. An `allow` rule for the server, an answer 2
given for the tool before the flag was added, and `--dangerously-skip-permissions` do not answer it.
A `deny` rule still refuses the call. A one-shot run, which has nobody to ask, and a delegate refuse
the call, and the desktop application refuses it without showing a card, telling the model and you
that it cannot ask and that the terminal can. The desktop application's connector page says under
the connector that its calls are asked about, and keeps the setting when you edit the connector.
Adding the flag to a server you already approved asks the approval question again, with
`ask_every_call` named as the field that changed. `get`, `list`, `doctor` and the approval question say the server asks every call.

### Arguments that take a choice

A choice is the number a processor picked from a numbered list. `--choice <tool>:<argument>` names
an argument of one of the server's tools that a choice may fill, once for each pair:

```sh
bravebot mcp add brave-input --ask-every-call --choice click_element:element_number \
  -- /path/to/bravebot-browser mcp --group input
```

Only a program on this machine that is also added with `--ask-every-call` can name one. A server
reached by `--http`, and a server that is not asked about every call, are refused with a sentence
that says so, and nothing is written. A tool and an argument are each letters, digits, `-` and `_`, an argument at most 60 characters.
`get`, `list`, `doctor` and the approval question list the pairs as `tool:argument`. Adding a pair
to a server you already approved asks the approval question again, with `choice_arguments` named as
the field that changed. The desktop application's connector page keeps the pairs when you edit the
connector. `bravebot-browser install --groups input` prints the declaration of the browser's `input`
group with a pair for each of its tools that takes the number of an element.

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
the list, and records nothing: a later run without it asks each question again. A call to a server
added with [`--ask-every-call`](#servers-that-ask-every-call) is still drawn and asked.

## Running a server's prompts

A server that has prompts offers them in the list a slash opens, among the skills. Each row is
`/mcp__<alias>__<name>`, with what the server says about the prompt and `(MCP <alias>)` after it:

```
> /mcp__
  ❯ /mcp__github__review_pr  Review a pull request (MCP github)
```

Once the name is typed in full, the box shows what the prompt takes: `number=<value> [focus=<value>]`,
where an argument in brackets is optional. Taking the row from the list writes the name of each
argument that is not optional, as `number=`, for you to give a value. Type the arguments after the
name as `name=value`, with a value that holds spaces in double quotes:

```
/mcp__github__review_pr number=456 focus="error handling"
```

Enter sends the line, and the turn fetches the prompt from the server with the arguments you typed.
Nothing is asked, since the line is yours and holds the arguments. What comes back is untrusted and
private, as a tool's result is: the model is handed a reference to it and none of what it says, and
running a prompt does not vouch for it.

- The list is read when the server starts, and only from a server that said in its handshake that it
  has prompts. A prompt the server adds later is not offered until it starts again.
- A row is offered at the start of a line, and nowhere else, since a line runs a prompt only from
  its first word.
- A word after the name that is not `name=value`, a name given twice, and a quote that does not
  close are said, and the line stays in the box and is not sent.
- A [rule](#rules) that denies `weather:forecast` also refuses a prompt named `forecast` of that
  server, before anything is sent.
- A prompt file whose text would run a prompt is refused, so no file runs one.
- Where two of your aliases both begin the name, as `a` and `a__b` do for `/mcp__a__b__c`, the line
  names neither and is not sent.
- A server that lists prompts it cannot offer, because a name is not letters, digits, `-` and `_` or
  is listed twice, is said at the start, with how many. A server that rejects the request for its
  prompts keeps its tools and says so.
- Only text is taken from a prompt, and a message's role is not read.

## What a server can reach

A program server is started confined, and on a platform with no confinement for one it is not
started. On Windows the confinement is a container, and a program named without an extension, such
as `npx`, is looked for as `.com` and `.exe` first, so `npx` starts as `npx.exe`. `.bat` and `.cmd`
files are not started. It gets:

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
  if you gave none. A directory inside a git repository is refused, since git runs the commands a
  repository's configuration and hooks name, and a server that may write them may run code
  unconfined the next time you run git there. On macOS the server cannot write a `.git` anywhere;
- the network by IP address, unless the session closes the network or limits it to a list of hosts.
  Where the sandbox network is closed (the `read-only` mode, for one), the server gets no network at
  all, a runner's fetch of its own package included. Where `sandbox.network.allowedHosts` is set, or the server's declaration has an `allowed_hosts` list, the
  server is held to the proxy that applies the list, plus the registry hosts of its toolchain. A
  platform that can apply neither does not start the server, and the note names the setting. The
  server also gets the machine's own system directories. A local socket, such as
  the one the Docker daemon listens on, is reached only in a directory the server may write, so a
  server that runs `docker` does not reach the daemon. On Linux that needs a kernel with Landlock
  ABI version 9, and an older one leaves every socket reachable;
- a look at any path, which says whether something is there and what kind of thing it is, and
  not what a file holds or what a directory lists.

It does not get your home directory, other than a `PATH` entry inside it such as `~/.local/bin` and
the installation its program came from, and it does not get the workspace unless `--dir` names it.
Where the platform cannot grant a path that is not on disk, the server is started without it and the
session names the server and each path left out. A program named without a path is looked for
only in the `PATH` you gave: without `-e PATH` it is not found, and the session says to name it
or give the program's full path.

A program kept in a directory a local server may write, such as the one you gave with `--dir`, is not
found when bravebot looks a program up by name for something else, such as an editor, a hook or a
clipboard tool. A program named by its full path is started as before.

A service server is reached through the same network gate as everything else the session sends. A
redirect off the scheme, host and port you declared is not followed, so a redirect from `http` to
`https` on the same port is asked about too. Nothing is sent there, and you are
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
only then rewrites the entry in `~/.bravebot/mcp.json` and approves it. The entry changes its
address and nothing else: its tool filters, its header and bearer variables, its time limits and
`--ask-every-call` stay as you wrote them. Its list of tools is put to
you again when a session next starts it, since the list you said yes to was read at the old
address. A yes does not move it to an address a declaration cannot hold, or to a host your
organization's settings refuse. A no changes nothing: the call is refused, or the server is not
started. A one-shot run, a session with nobody at the terminal, and
`--dangerously-skip-permissions` refuse the redirect without asking.

## Connecting Gmail, Google Calendar, GitHub and Brave

Reading Gmail through Google's Workspace server, with sign-in done once outside bravebot and the
tools that send mail refused, has a page of its own: [Connecting Gmail](mcp/gmail.md). Reading
Google Calendar through the same server, with the tools that change a calendar refused, is
[Connecting Google Calendar](mcp/calendar.md). Reading GitHub issues and pull requests through
GitHub's own server, with every tool that writes refused, is [Connecting GitHub](mcp/github.md).
Using Brave's tabs, history, bookmarks and more through the BraveBot extension, one group of tools
at a time and each off until you turn it on, is [Connecting Brave](mcp/brave.md). Further pages for
particular servers will sit beside them if they read naturally.

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

A server that runs on your machine has a `Server policy` heading at the end of the report, with what
its confinement allows it: the network, whether it may start subprocesses, where it starts and one
line for each path it may read or write. It is what to check a refusal from inside the server
against:

```
  Server policy  weather
  egress         any address
  subprocesses   allowed
  starts in      /Users/you/work
  .git           not written under a write row
  read           /usr/lib
  read           /usr/bin
  read, write    /dev/null
  read, write    /var/folders/v6/0scctxs13mb3nbg5y3t46w4r0000gn/T
  read, write    /Users/you/.bravebot/mcp-home/31e1d0473746eba39a0e9e04e1105ef23298573af672413c346738fadf234f4f
  read           /Users/you/servers/weather
  read, write    /Users/you/work
```

On Linux the `.git` line is absent, and the base lists the system directories of that platform.
`left out` heads the paths that were wanted for the server and are not on disk, on a platform that
grants no path that is not, with the reason beside it. A server reached over HTTP runs no process
here and has no such lines.

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
answer 2 at the server question and the tools answer 2 stopped asking about. A server added with
[`--ask-every-call`](#servers-that-ask-every-call) has a line saying so above those. The rows are
for a session at a terminal. Where nobody can be asked, a server that would be asked about is not
started. A server a file here asks for and nobody declared is a row of its own, ending with the
command that declares it. An entry that cannot be used, because it was edited by hand into
something a declaration cannot be, is listed with what is wrong with it, and the list then fails so
that a script notices. A url with a `:` and no port, written into the file by an earlier version, is
treated the same way: it is kept as it was written and listed as unusable, and editing the url in the
file is the way to fix it.

`bravebot doctor` prints the same rows and fails where `list` does.

`bravebot mcp get weather` shows one declaration in full, with the whole digest:

```
declared in /Users/you/.bravebot/mcp.json
  weather   stdio   npx -y @dangahagan/weather-mcp@latest
            variables: PATH
            digest: 25edc5e806956f3254b5ff2b116f1c07ae5ad8d4a5df00f6d15c0f8aadfec43f
            approved
            a session here would confine it to:
              egress        any address
              subprocesses  allowed
              starts in     /Users/you/work
              read          /usr/lib
              read, write   /Users/you/work
              ...
```

For a server that runs on your machine, `get` ends with the confinement a session would start it
under, in the words `/status` uses for one that is running, so you can read what a server would be
allowed before you approve it. Nothing is started to show it. Under an allowed-hosts
list the egress line says the server would be held to the proxy that applies the list. The server's
own directory is not made: where it is kept under your state directory its path is listed, and where
the session may write nothing there it is a new directory in the temporary directory and the line
says so. A server a session would not start has the sentence the session says for it, and a server
reached over HTTP has no such lines.

## Removing one

```sh
bravebot mcp remove docs
```

Removes the declaration and its approval together. An approval another declaration still resolves
to is kept. A settings file that asks for it still does, and is reported as asking for a server
nobody declared: `bravebot mcp disable docs` takes that out. In the full-screen interface,
`/mcp remove docs` does both.

## Refused by an administrator

The machine's [administrator file](managed-settings.md) can keep a server
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

An [incognito session](../using/incognito.md) writes nothing
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
- **A directory holding checkouts is not refused.** Only the directories above `--dir` are searched
  for a repository, so a server given one above your projects may write their files. Give a server
  a directory of its own.
- **On Linux a server can write a repository it reaches.** A `.git` under its directory, its home
  or the temporary directory is one it may write, whether it was there already or the server made
  it, so do not run git in a directory a server has written.
- **On macOS a server cannot clone a repository.** It cannot write a `.git` anywhere, so a server
  or runner that clones one into its home or the temporary directory fails, and no server can be
  given a git repository to work on.
- **If your home directory is a git work tree, every directory in it is refused as a server's.**
- **A Windows server's grants change the filesystem.** A grant for the server's home, its directory
  and the directories its program is found in changes the access list of each, so a run that ends
  without cleanup leaves the entries behind. A server there may also start a subprocess, since the
  container has no way to deny one.
- **The desktop application starts a session's servers with its first turn or its first plan run,
  not as the session opens.** That turn or run waits for the question about each server and for every
  handshake before the model is asked anything, and the window says it is starting servers while it
  does. A turn or run stopped while they start starts none, and the next one asks again. A reopened
  or forked session starts its own, since what a session started is not written to its record, and a
  server's own error output is discarded.
- **A prompt takes named arguments only.** `/mcp__weather__forecast Paris` is refused, since taking
  the first argument's name from the server's list would make a word it sent decide what your line
  means. Taking the row writes the names of the arguments that are not optional, so you type their
  values, and the row shows the optional ones.
- **The desktop application and `--plain` do not offer a server's prompts.** Only the terminal
  client's box lists them and runs them.
- **A checkout cannot bring its own server.** A project that needs one says so in its README, and
  each person declares it. That is the point, and it costs a step per machine.
- **An approval does not travel.** It lives in your own directory, so a second machine asks again.
  The same is true of a list you said yes to and a tool you stopped the asking for.
- **`--plain` cannot stop asking for a tool.** Its call question has room for one answer.
- **A call with private arguments cannot be stopped from asking.** Answer 2 is not offered for it, because no stored answer covers it.
- **One answer covers several calls in a round.** Calls to the same tool made in one round are listed in one question, so a yes also makes the later ones.
- **What the model has read can leave in a URL or a command.** Once you let a result out of
  quarantine, every call to a server is put to you, but a fetch to a host an `allow` rule covers is
  not, and nor is a command a rule allows: a URL and a command line are not held back as private.
- **A server that holds nothing of yours still asks.** Every result is private, so a search
  server's answer fed to a command is put to you as a mailbox's would be.
- **A one-shot run still checks a list it then refuses.** The check is a model call nobody reads
  the answer to, made once per run for each server whose list you have not said yes to.
