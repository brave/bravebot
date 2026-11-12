---
sidebar_position: 4.5
title: The usage ping
description: The one request a day that tells Brave a copy of bravebot ran, everything it holds, what it never holds, and every way to turn it off.
---

# The usage ping

Once a UTC day bravebot sends Brave one request saying that a copy of it ran. Brave uses it to count
how many people use each release, on which systems and through which front end. It is on unless you
turn it off, and nothing is sent until bravebot has told you about it once.

## What is sent

One `GET` request, with these fields and nothing else:

| Field | What it holds |
| --- | --- |
| `platform` | `darwin`, `linux` or `windows` |
| `version` | the version of bravebot, such as `0.21.0` |
| `install` | how it was installed: `npm`, `script`, `powershell`, `homebrew`, `winget`, `apt`, `dnf` or `unknown` |
| `surface` | `tui` for the terminal interface, `desktop` for the desktop app, `cli` for `-p`, `--plain` and `bravebot acp` |
| `backend` | the kind of model service the launch starts with: `brave`, `bedrock`, `chatgpt`, `xai`, `openai`, `anthropic`, `openrouter`, `ollama`, `lmstudio`, `llamacpp`, `other` or `none` |
| `premium` | `true` if your configuration holds a premium key for Brave's endpoint |
| `daily`, `weekly`, `monthly` | `true` if this is the first ping this UTC day, week (from Monday) or calendar month |
| `first` | `true` until Brave has accepted one ping from this install |
| `woi` | the Monday of the week of the first ping Brave accepted from this install |

A first ping from the terminal on a Mac looks like this:

```text
https://laptop-updates.brave.com/1/usage/bravebot?platform=darwin&version=0.21.0&install=unknown&surface=tui&backend=brave&premium=false&daily=true&weekly=true&monthly=true&first=true&woi=2026-11-09
```

The request carries a user agent of `bravebot` and no credential or cookie. Brave's service adds the
country and region it reads from the network address and the date it received the ping. It drops
pings from data centres, and in a country with few users it blanks the region and `woi`.

## What is never sent

No prompt, reply, file, path, file content, key, token, account name, host name, model name, gateway
name, working directory, or identifier that is the same from one install to the next. Every value is
a word from a fixed list, a boolean or a date, so nothing you typed can reach the request. A gateway
whose name is not one of the services listed above is sent as `other`.

The service sees the network address the request came from, as any server does.

## When it is sent

At most once per UTC day for each `~/.bravebot`, whichever of the terminal, the desktop app or a
command line run starts first that day. A ping that fails is tried again by the first launch at least
an hour later. A scheduled or triggered run sends none, and neither does a session started with
`--incognito`.

## Turning it off

Any one of these is enough:

- In the terminal, open `/config` and turn **usage ping** off.
- In the desktop app, open **Settings → General → Privacy** and turn off **Send a daily usage ping to
  Brave**. This and `/config` write the same file, `~/.bravebot/usage-ping`, so turning it off in one
  turns it off in both.
- Add `"usagePing": false` to your own `~/.bravebot/settings.json`. A checkout's
  `.bravebot/settings.json` is not read for this.
- Set `BRAVEBOT_USAGE_PING=0` in the environment.
- Set `DO_NOT_TRACK` in the environment to anything but an empty value, `0` or `false`.
- An administrator can add `"usagePing": false` to the [managed file](../customize/managed-settings.md),
  which nobody's own setting overrides.

When the ping was turned off by a setting, a variable or an administrator, `/config` and the desktop
show it as off and say which one did it.

## Seeing it for yourself

Set `BRAVEBOT_USAGE_PING_URL` to the address of a server of your own, such as
`http://127.0.0.1:8080`, and the ping goes there instead, with the same path and fields. A build from
source sends nothing unless this is set.

See [`usagePing`](../customize/configuration.md#usageping) for the setting.
