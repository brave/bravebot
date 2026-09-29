---
id: BROWSER
title: Reaching the browser through an extension
status: proposed
governs:
  - docs/design/browser-extension.md
documented-by: none (gap: a "Connecting Brave" section of docs/website/docs/customize/mcp-servers.md, once this is built)
---

## Scope

How BraveBot reaches a Brave extension that reads pages and profile data for it: the processes
between the two, who starts each, how they find and trust each other, and what is sent between
them. The design and its reasons are [browser-extension.md](../design/browser-extension.md). What
an MCP server may do once it runs is [mcp.md](mcp.md) and [mcp-servers.md](mcp-servers.md), and
nothing here changes either.

## What exists today

Nothing. Every clause below specifies work not yet done, and is `verified-by: none` until it lands.

## The processes

```
BraveBot ──MCP──▶ bravebot-browser mcp ──socket──▶ bravebot-browser native-host ◀──▶ extension
                  (started by BraveBot,             (started by Brave,           native
                   confined)                         not confined)               messaging
```

One program, `bravebot-browser`, runs in both roles. BraveBot starts it as a stdio MCP server,
declared with `bravebot mcp add` like any other. Brave starts it as a native messaging host when the
extension connects. The two copies talk over a Unix socket.

## What the confinement allows

A stdio server runs confined. How that confinement treats a Unix socket decides which side listens
and where the socket is. Measured against the profile each platform's confinement writes for a
stdio server with egress and a `--dir`:

| From inside the confinement | macOS (Seatbelt) | Linux (Landlock ABI 8) |
|---|---|---|
| connect, socket in `--dir` | allowed | allowed |
| connect, socket outside every grant | allowed | allowed |
| connect, egress withheld | refused | not measured |
| bind, in `--dir` | refused | allowed |
| bind, outside every grant | refused | refused |

On macOS a connect to a Unix socket is a network operation and a bind is refused outright. On Linux
the right that governs connecting to a pathname socket is Landlock ABI 9, which the confinement
handles where the kernel carries it, so on such a kernel a connect outside the write grants is
refused. That row was not measured, because the kernel tried carries ABI 8.

## Clauses

<a id="BROWSER-1"></a>
### BROWSER-1: the native host listens and the MCP server connects

The copy Brave starts creates the socket and accepts on it. The copy BraveBot starts connects to it
and never creates one.

**Why.** A confined server cannot bind a Unix socket on macOS, in any directory. Connecting out is
allowed on both platforms.

`verified-by: none`

<a id="BROWSER-2"></a>
### BROWSER-2: the socket is in the directory the server's declaration names

The socket is created in one directory, made by the person's account with no access for anyone
else, and the MCP server's declaration names that directory with `--dir`.

**Why.** On a Linux kernel carrying Landlock ABI 9, a confined process connects only to sockets
under its write grants, and `--dir` is the one write grant a declaration adds. A socket anywhere
else works on today's kernels and stops working on a newer one.

`verified-by: none`

<a id="BROWSER-3"></a>
### BROWSER-3: the native host serves only a peer holding the secret in that directory

The native host writes a random secret into the socket's directory as it starts, readable by the
account alone. A connection that does not present it first is closed before anything is forwarded.
The host also refuses a peer whose user id is not its own.

**Why.** The confinement does not keep other processes off the socket. Every stdio server BraveBot
starts has egress, and on macOS egress is what reaches a Unix socket, so any of them can connect.
Only a process granted the directory can read the secret, and only the declaration naming it is
granted it.

`verified-by: none`

<a id="BROWSER-4"></a>
### BROWSER-4: the native host speaks only to our extension

The host manifest's `allowed_origins` names one extension id. The host also checks the caller origin
Brave passes it and exits when it names any other extension.

**Why.** `allowed_origins` is enforced by the browser. Checking the origin again in the host keeps
a manifest someone edited from handing the relay to a different extension.

`verified-by: none`

<a id="BROWSER-5"></a>
### BROWSER-5: the socket exists while the extension is connected

The native host runs from the extension's connect until its port closes, and removes the socket and
the secret as it exits. While no extension is connected there is no socket.

**Why.** Brave owns the host's lifetime and nothing else can start it. A socket left behind would
point a connecting server at a relay that is not there.

`verified-by: none`

<a id="BROWSER-6"></a>
### BROWSER-6: a call with no extension connected fails at once

A tool call made while the socket is absent, refuses the connection, or closes before the reply is
a failure naming the reason: Brave is not running, the extension is not installed, or it
disconnected. It is never an empty result and never waits for the extension to appear.

**Why.** An empty result would read as "the page is empty" or "no history matches". Waiting would
hold the turn on something only a person can fix.

`verified-by: none`

<a id="BROWSER-7"></a>
### BROWSER-7: each reply reaches the session that asked

Several BraveBot sessions may be connected to one native host at once. The host rewrites each
request id so it is unique across its connections, and hands each reply back only on the connection
the request came in on, with the original id.

**Why.** There is one extension and one native messaging port. Two sessions each numbering requests
from 1 would otherwise receive each other's replies.

`verified-by: none`

<a id="BROWSER-8"></a>
### BROWSER-8: no message to the extension exceeds the platform limit

A message from the host to the extension is at most 1 MB, the limit native messaging sets. A reply
from the extension may be up to 64 MB. The host refuses a request it cannot fit rather than sending
it and having the port closed.

**Why.** Brave closes the port on a message over the limit, which disconnects every session at once.

`verified-by: none`

<a id="BROWSER-9"></a>
### BROWSER-9: installing writes one manifest in the locations Brave reads

`bravebot-browser install` writes one host manifest under a host name of our own, into the
per-user location Brave reads for its stable channel:

| Platform | Where |
|---|---|
| macOS | `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` |
| Linux | `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` |
| Windows | the registry key `HKCU\SOFTWARE\Google\Chrome\NativeMessagingHosts\<name>` |

It writes nothing else there and changes no other host's manifest.

**Why.** On macOS and Windows Brave reads Chrome's locations rather than its own, so a manifest in a
Brave directory there is never read. Chrome reads the same file, and `allowed_origins` is what keeps
Chrome from starting the host for anything but our extension. On Linux the location is under Brave's
own profile directory, and a channel other than stable has its own.

`verified-by: none`

<a id="BROWSER-10"></a>
### BROWSER-10: what the extension returns is labelled like any server's result

Page text, history and bookmarks reach BraveBot as an MCP result and nothing more. They are
untrusted and private, as every server's result is.

**Why.** Page text is written by whoever wrote the page, and history is the person's own. Nothing
about the route through an extension makes either one safer to read.

`verified-by: none`

## Known costs

- The secret in [BROWSER-3](#BROWSER-3) is only as private as the directory. Any process of the
  person's that can read it can use the relay, which is the same account boundary a keychain gives.
- A confined server can connect to the socket only while it has egress. A declaration that withholds
  egress, where a future version allows one to, cannot reach the relay on macOS.
- Linux with Landlock ABI 9 was reasoned about and not measured.
- Windows starts no stdio server today, so the MCP half cannot run there at all, and whether a named
  pipe or a Unix socket is the right transport is open.
