---
sidebar_position: 10
title: Incognito sessions
description: Start a session with --incognito so it adds nothing to ~/.bravebot, and see what is still read and what still reaches the filesystem.
---

# Incognito sessions

```sh
bravebot --incognito
```

An incognito session runs like any other and adds nothing to `~/.bravebot`, apart from the four files
[below](#what-it-does-not-cover). No prompt reaches the history, no session record and no title are
written, no audit trail is kept, and a model, theme or effort level chosen inside it applies for
that session without being recorded. The sessions directory is not created either, since an empty
one still says that a session ran, in this project, at this time. No record is written, so nothing
is resumable. An incognito session does not appear in the picker, including to itself.

A history and records that were already there are left exactly as they were, and an ordinary session
from before stays resumable. It stops being updated for as long as the incognito one runs.

**Reading is untouched.** The settings, the model and theme you chose, your standing instructions,
your skills and your imported credentials are all read as usual, so the session is the one you
configured rather than a fresh install. The record of command lines you asked to be remembered past a
session is read on the same terms: a line already in it still stops the asking, and the key that would
add one is not offered. So is the record of directories you answered yes to at the startup question,
which is [kept](../security/trust.md#remembering-the-answer): a kept yes still trusts the directory,
the session says so as it opens, a new yes is not kept and the question says so, and `/forget-trust`
changes nothing and names the file so you can remove it yourself.
The record of AWS Bedrock models that refused the request for their thinking to be shown is read too,
so a model known to refuse it is not sent it, and a refusal this session finds is not written.

**One thing is not read: your standing answer about [vetting](../security/vetting.md).** If you have
pressed the key that stops bravebot asking before content nobody vouched for reaches the planner,
an incognito session asks anyway. A model and a theme decide what a session looks like, and that one
decides whether you are asked, which is not a decision to inherit into a session you asked to leave
nothing behind. `--vet` and the `vetting.auto` key in your settings still work here, so you can say
what you want of this run.

The flag may go anywhere in the command line and combines with `-p`, `--resume`, `--mode` and a
bare invocation alike. It cannot be turned off once the session has started.

## What it does not cover

Eleven things still reach the filesystem:

- **Your project.** `write_file` and `edit_file` go on editing it. Those edits are the work rather
  than a trace of it.
- **Programs you run.** A program reaches the filesystem with the access your own shell would give
  it, and may write whatever it likes. What confines one is
  [confinement](../security/security.md#confinement), which is a different question.
- **The editor hand-off.** Composing in `$EDITOR` writes a scratch file. It goes to the system
  temporary directory rather than `~/.bravebot`, is readable by nobody else, and does not outlive the
  edit.
- **The session's own scratch directory.** Somewhere to put a file that is not part of the project is
  something a turn needs, and this mode does not take it away. It sits in the system temporary
  directory on the same terms as the editor's file and goes with the session. Its name says which
  program made it and nothing about which project or which session, so an empty one records only that
  this program ran at this time.
- **A language server's index.** A server indexes the workspace before it can answer anything, and
  the index is too large to hold in memory. It goes to a directory of its own in the system temporary
  directory and goes with the session, so the next session indexes again. It is never written into the
  project.
- **A local MCP server's home.** A [server](../customize/mcp-servers.md) is given a directory of its
  own to keep its files in, such as a runner's cache. Here it sits in the system temporary directory
  on the same terms and goes once the server stops, so a runner fetches its package again each
  session.
- **A credential you spent.** With a [Leo Premium](../customize/premium.md) subscription imported,
  the session spends credentials from it as any session does, and records which ones it spent in the
  file the import created under `~/.bravebot`. A credential is single use and presenting it to the
  service spends it there, so one left looking unspent would be offered again in your next session,
  wasting a credential you paid for. If the batch runs out while you are working, a new one is
  fetched for the same subscription and written to the same file, rather than thrown away when you
  exit. What ends up in the file is credentials for a subscription you imported before the session
  and which of them are spent, and nothing about the project or what you asked.
- **A ChatGPT sign-in whose access token was replaced.** A stored
  [ChatGPT sign-in](../customize/providers/chatgpt.md) is used as in any session. When its access
  token is replaced the new tokens are written to `chatgpt-account.json` under `~/.bravebot`, and a
  sign-in OpenAI has ended is removed from it. OpenAI replaces the refresh token each time it is used,
  so tokens that were not written would leave the file holding one that no longer works, and your next
  session would find the sign-in ended. What ends up in the file is the tokens of a sign-in you made
  before the session, and nothing about the project or what you asked.
- **An xAI sign-in whose access token was replaced.** A stored [xAI sign-in](../customize/providers/xai.md)
  is used as in any session. When its access token is replaced the new tokens are written to
  `xai-account.json` under `~/.bravebot`, and a sign-in xAI has ended is removed from it. xAI replaced the
  refresh token when it was used, observed once with one account, so tokens that were not written could
  leave the file holding one that no longer works, and your next session would find the sign-in ended. What
  ends up in the file is the tokens of a sign-in you made before the session, and nothing about the project
  or what you asked.
- **The copy of a picture you are asked to open.** When a [vetting](../security/vetting.md) prompt is
  about a picture, you are handed a copy to open in your own viewer. It is written to
  `bravebot/vetting` under your cache directory, readable by you alone, and removed when the prompt
  closes. A process killed with the prompt open leaves it behind.
- **A memory left untrusted.** Where the session writes something nobody vouched for into a
  [delegate definition's memory](../customize/agents.md#memory),
  the path is recorded under `~/.bravebot/untrusted`, so that your next session does not read it as
  trusted. The record names the file and nothing else, though that does say you worked in the
  project.

[`import-leo-creds`](../customize/premium.md) is refused rather than quietly skipped, since a
credential that did not outlive the session would not be an import. `--forget` still works, because
removing a stored secret leaves less behind rather than more. `bravebot auth login chatgpt` is refused
the same way, before a browser opens, and `bravebot auth logout chatgpt` still works. `bravebot auth login xai`
is refused the same way, before the code is asked for, and `bravebot auth logout xai` still works.
