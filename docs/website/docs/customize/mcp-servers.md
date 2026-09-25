---
sidebar_position: 7
title: MCP servers
description: Declare a Model Context Protocol server, approve it, and see what is declared.
---

# MCP servers

A Model Context Protocol server is a program of somebody else's, or a service somewhere else, that
offers tools. Before one can be used you declare it, which says what it is, and you approve it,
which says you have read what it will run.

:::note
**This build declares and approves servers and does nothing else with them.** No session starts a
declared server or offers its tools yet, so an approved server is a line in a file and no more.
What follows is the part that is built.
:::

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
argument.

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
line, to `~/.bravebot/mcp-approved`.

So changing anything a server would run is a server nobody approved. Pinning the version above:

```
  weather   stdio   npx -y @dangahagan/weather-mcp@1.4.0
            variables: PATH
            digest: a3042003
            changed: argv

  Use this MCP server? [y/N]
```

The old approval is gone whatever you answer, since nothing declares what it approved any more.

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

## Where nothing is written

An [incognito session](../using/sessions.md#a-session-that-leaves-nothing-behind) writes nothing
under `~/.bravebot`, so `add`, `approve` and `remove` are refused in one. On a machine that names no
profile directory there is no `~/.bravebot` at all: nothing is declared there, and nothing can be.

## Known costs

- **Nothing uses a declaration yet.** Declaring and approving are built ahead of the session that
  would start a server, so that what the question asks is settled before anything depends on it.
- **A checkout cannot bring its own server.** A project that needs one says so in its README, and
  each person declares it. That is the point, and it costs a step per machine.
- **An approval does not travel.** It lives in your own directory, so a second machine asks again.
