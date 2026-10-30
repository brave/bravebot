---
sidebar_position: 6
title: Sandbox settings
description: Withhold variables from a program the agent runs, close its network or hold it to a list of hosts, and move the paths it reads and writes.
---

# Sandbox settings

These settings change what a program the agent runs is handed, which hosts it may reach and which
paths it reads and writes. They go in the [settings files](../customize/configuration.md#settingsjson),
which that page describes along with how the files combine.

How a line is compiled, approved and run is in [the run tool](../reference/run-tool.md), including
[what a program is handed](../reference/run-tool.md#what-a-program-is-handed). The sandbox modes and
what a confined program cannot reach are under [Confinement](security.md#confinement).

## `run.scrubEnv`

```json
{ "run": { "scrubEnv": ["MY_TOKEN", "OTHER_SECRET"] } }
```

Variables to withhold from a program the agent runs, on top of bravebot's own credentials, which are
withheld with no configuration at all. Those credentials are the signing key, its key id and the crash report submission URL, and every
variable a [provider block](../customize/providers/openai-compatible.md) names in its `env` as the place a gateway's token is read from,
so a gateway token held in `OPENROUTER_API_KEY` or in a name of your own is withheld without being
listed here. A variable no provider block names is yours and stays. A block you add mid-session
withholds its variable from the next program started. See [`run`](../reference/run-tool.md#what-a-program-is-handed).

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

**An entry may be a pattern.** `*` stands for any run of characters and `?` for exactly one, matched
against the whole of a variable name and ignoring case, so `AWS_*` withholds `AWS_PROFILE` and
`aws_region` and leaves `PATH` and `MY_AWS_KEY`. `*` and `?` are the whole of the syntax. A pattern
matching nothing withholds nothing; an exact name is withheld whether you have that variable set or
not. `bravebot doctor` lists each pattern with one variable it matched on this machine, names only,
so you can see whether it caught what you meant. An exact name is not listed.

```json
{ "run": { "scrubEnv": ["AWS_*", "*_TOKEN", "MY_SECRET"] } }
```

Nothing is withheld by a pattern you did not write. bravebot adds none of its own, because a rule
matching variable names cannot tell `run aws s3 ls` from an exfiltration.

`BRAVEBOT_SUBPROCESS_ENV_SCRUB=0` turns the withholding off entirely. Only that exact spelling does
it: `false`, `no` and `off` change nothing, because a credential reaching every subprocess is not a
thing to switch off by near-miss.

## `run.network`

```json
{ "run": { "network": "closed" } }
```

`open` is the default: every program the agent runs keeps the network. `closed` takes it from every
program except those that carry a reason to have it: a package manager that fetches (`cargo`, `npm`,
`pip`, `go`, `mvn`, `gradle`), a `git` or `gh` command that talks to a remote, `curl`, `ssh`, and
a command that was lent a credential (remote, cloud, cluster or container). A program the agent could have written itself, under a directory it may write to, gets nothing by being named `curl`. Those directories include the toolchain caches and every path you let programs write. `cat`, `grep`, `make`, `python3` and a test
run get none, so what they read cannot be sent anywhere.

`--run-network closed` sets it for one run, and a machine-level file can pin it, which no other
layer overrides. A project's own settings can close the network and cannot open it. On Linux a
program with no network runs in a network namespace of its own, which needs a kernel that lets an
unprivileged user make one. Ubuntu 24.04 restricts that by default and many containers block it.
Where it is unavailable, a program that would lose the network is not started and the result says
so, and `bravebot doctor` says why. The opening screen,
`/status` and `bravebot doctor` say when it is closed and which layer closed it.

The desktop app reads the setting once, when it starts, from your own file, the file `--settings` names
when it exists, the settings file of the directory it was started in and the machine-level pin. A project opened in a window afterwards, or a
settings file chosen there, does not change it.

## `sandbox.network`

```json
{
  "sandbox": {
    "network": {
      "allowedHosts": ["registry.npmjs.org", "*.github.io"],
      "deniedHosts": ["evil.example"],
      "onUnlisted": "refuse"
    }
  }
}
```

Holds the programs the agent runs to a list of hosts while the network stays open for everything
else. `allowedHosts` names the hosts a program may reach, with GitHub added for a `git` or `gh`
command that talks to a remote and a toolchain's own registry added for `cargo`, `npm`, `pip` and
`go`. `deniedHosts` refuses hosts whatever else allows them. `onUnlisted` is `refuse` or `ask`: a host
no list names is refused, or put to you. With `ask`, the program is refused at once and the hosts it
was refused are put to you after that run has stopped, up to eight at a time, one per row. A yes adds
them to the allowed set until the session ends and writes nothing to disk, so you run the line again;
a no is kept for the session and the host is not asked about again. The terminal, the plain CLI, the
desktop app and an [editor](../using/editors.md) all ask, and an editor's approval is labelled as
lasting the session. A host on `deniedHosts`, one
refused for its port or as not public, and anything a delegate's line asked for are never put to you,
and bypass mode and a session nobody is at refuse as `refuse` does. `/clear`, `/branch` and `/resume`
begin a session that asks about those hosts again. A stage with the network points its proxy settings at a local proxy that applies the list, and
on macOS and Linux it may connect only to that proxy, so a program that ignores the proxy settings
is still held to the list. On Linux that needs the same kernel support as `run.network`, and a stage
is refused, naming this setting, where the kernel lacks it. On Windows a stage is refused while a list
is set, naming this setting. A `run` that asks for `loopback` is then given the right to listen only. A list that is set,
even an empty one, is a list, and an
empty `allowedHosts` refuses every host. See [the sandbox](security.md) for what the list
can and cannot hold back.

`deniedHosts` and `onUnlisted: refuse` only take reach away, so any file may write them. `allowedHosts`
and `onUnlisted: ask` give it back, so they are read from your own file and a `--settings` file
outside the project; a project or local file's are ignored and named by `bravebot doctor`. A value
that is not a list of strings, or an `onUnlisted` that is neither word, is reported and read as absent.

A machine-level file pins what it writes. Its `allowedHosts` is the whole allowed set and nothing you
wrote is added to it, its `deniedHosts` is added to yours, and its `onUnlisted` is the answer whatever
you said. A pinned `allowedHosts` also drops your `onUnlisted: ask` unless the file sets `onUnlisted`
itself. A pinned value that cannot be read pins nothing, and `bravebot doctor` names it.

## `sandbox.filesystem`

```json
{
  "sandbox": {
    "filesystem": {
      "allowWrite": ["~/notes"],
      "denyRead": ["~/.config/gh", "**/*.env"],
      "denyWrite": [".env"],
      "allowRead": ["~/.aws"]
    }
  }
}
```

Four lists of paths for the programs the agent runs. `allowRead` lets them read a place that is
refused by default, such as `~/.aws`. `denyRead` refuses them a place they could read. `allowWrite`
lets them write and read a place. `denyWrite` takes write access away under a path, one inside a
directory the session was opened on included. Each list is empty unless you write it.

A path is absolute, starts with `~/`, or is relative to the directory the session was opened on. A
`..` that climbs out of it is refused. `*` and `?` work in `allowRead` and `denyRead`, so
`**/*.env` refuses every file ending in `.env` under the directory; the listing is made when a
command starts, so a file made afterwards is not in it. A link is judged by where it leads.

The narrower path wins: `allowWrite` on `~/a` and `denyWrite` on `~/a/b` writes `~/a/c` and not
`~/a/b/x`. Where both name the same path the refusal wins, and it wins over what a command is given
for itself, so a refusal of `~/.config/gh` stands against `gh`. Nothing adds reach to `~/.ssh` or
`~/.bravebot` or to anything inside either, except that an `allowRead` entry may name
`~/.bravebot/sessions` or a path inside it, which programs already read and never write. The home directory, `/` and a directory above the
home are refused as write paths. On Windows an `allowRead` or `allowWrite` entry that is a directory
holding a credential location beneath it, such as the home directory, a directory above it or
`~/.config`, is refused; a wildcard entry drops those matches and keeps the rest. Linux and macOS
are unchanged.

Your own file and a `--settings` file outside the project may write all four. A project or local file
may write `denyRead` and `denyWrite` and not the other two, so a repository you clone cannot widen
what its own commands reach; `bravebot doctor` names the file that tried. A machine-level file can pin
any list: a pinned `allowRead` or `allowWrite` replaces yours, and a pinned refusal is one nothing
you wrote lifts. `--sandbox-allow-read`, `--sandbox-deny-read`, `--sandbox-allow-write` and
`--sandbox-deny-write` add to the lists for one run.

An entry that cannot be applied is not in force. A refused `denyRead` or `denyWrite` stops the
commands it would have held back, with a message naming it, rather than letting them through. On
Linux a directory holding a path you refused writes to cannot have new entries made directly in it,
and on Windows a command the refusal would reach is not started, since neither can narrow a write
grant.

The desktop app shows the lists in the conversation's permissions view, each entry with the file
that wrote it, and says in the notice above the conversation when an entry is not in force. It does
not edit them: change the settings file, and the next conversation reads it.

## `sandbox.holdBackTemporaryDirectory`

```json
{ "sandbox": { "holdBackTemporaryDirectory": true } }
```

Programs the agent runs can write the system temporary directory in every mode that confines. With
this set to `true` no stage writes it, so nothing a stage leaves there can be read or run by a later
program. The directory can still be read. The scratch directory and your own `allowWrite` entries are
separate and stay writable. Under `--sandbox off` the directory is refused too, although those
stages otherwise write anything your account can.

A program that makes a scratch file there is refused, such as a shell heredoc, `mktemp` or a
compiler's intermediate files, and the planner is told so in the `run` description. A `denyWrite`
entry naming the directory refuses it as well, and is the way to hold back one path inside it.

The key is read from your own file and a `--settings` file outside the project, as `sandbox.mode`
is. A project or local file that sets it is not obeyed and `bravebot doctor` reports the key as
unread. Any value but `true` asks for nothing.
