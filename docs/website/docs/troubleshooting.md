---
sidebar_position: 9
title: Troubleshooting
description: What to check when something is not working, and why some behaviour that looks broken is not.
---

# Troubleshooting

## Start here

```sh
bravebot doctor
```

It reports the endpoint, whether a premium host is configured, the key id, the model in force and
whether it was chosen or defaulted, the confinement available on this platform, and the state of any
imported subscription. It changes nothing, and a configuration error makes it **fail** rather than pass
with a warning.

Inside a session, `/status` reports the same session-level facts plus every trust rule and every
vouched-for command.

## Configuration

**`configuration error: …`**. The build is missing a required variable, or one in your environment is
wrong. The environment wins over what is baked in, so an exported `BRAVE_AI_CHAT_ENDPOINT` left over
from debugging will override a working release binary. See
[Configuration](customize/configuration.md#environment-variables).

**401 from the endpoint.** The services key is issued for exactly one environment, and used against
another it returns 401: the signature is well formed, the credential is not accepted there. For
a source build, check that `BRAVEBOT_ENV`, the key and the hosts all agree.

**A Leo Premium credential returns 401.** A credential only verifies against the deployment that
issued it. Import from the Brave channel matching the environment the binary is configured for. See
[Leo Premium](customize/premium.md#requirements-and-limits).

**`no model service is configured yet`**. Nothing is set up to send the request, so the run stops
instead of starting work that has no service to do it. It lists the ways to configure one, each
naming what to type or write. `doctor` says the same and fails. See
[Configuration](customize/configuration.md).

## When a turn fails

A failed turn is announced as `turn N failed` with a reason, and a turn you stopped yourself as
`turn N cancelled`, which is neither a success nor a failure. The reason stays on screen while you
scroll or resize.

The reason is drawn from a fixed set rather than from whatever the service said, so no error body,
header, URL or credential is ever quoted back at you. Where the status is known it is appended as
`(HTTP 429)`, and where requests were sent it says `after 3 attempts`, counting retries and
capability probes.

| What it says | What it means |
|---|---|
| the service would not accept the credentials | the credential is wrong for this deployment, or has lapsed |
| the service asked for fewer requests | rate limited, so wait and ask again |
| the service could not answer | the service is unavailable at the moment |
| the service rejected the request | the request was refused as invalid |
| the request did not get through | nothing reached the service: network, proxy or TLS roots |
| the reply stopped before it was finished | the reply was cut off part way |
| the reply could not be read | a reply arrived that could not be decoded, or the model sent back nothing twice in a row |
| the model reached its output limit | the reply hit a ceiling, which `BRAVEBOT_OUTPUT_BUDGET` raises |
| nothing here was configured to send the request | no model service is set up |
| a gate here would not let the request out | a gate refused it before it left, so it never went |
| the workspace could not be used | the working directory could not be read or written |

For the network cases, `bravebot doctor` has a `network` section naming the trust roots in force, the
proxy, and the hosts it is not used for.

**A turn failed but I was still charged for it.** That is deliberate. A later error or a stop does
not undo the cost of requests that already finished, so what completed work spent stays counted in
`/status` and in the session record. Requests that failed or never finished add nothing, and no
estimate is made for them.

**Every request failed after I picked a model through a gateway.** A gateway that will not accept an
effort level makes the turn give up the level and carry on without it, rather than refusing every
request the turn could make. If you are seeing the whole turn fail instead, you are on a version from
before that, so update.

## The interface

**Shift-Enter sends instead of starting a new line.** Use **Ctrl-J**, which always works, or use a
terminal that reports the modifier (Ghostty, Kitty, WezTerm) or configure yours to send a newline.
Most terminals send the same byte for Enter whichever modifier is held.

**Command-V pastes nothing when I copied a picture.** Use **Ctrl-V** for a picture. On Linux it needs
`wl-paste` or `xclip` installed. Command-V never reaches the process: the byte stream over a pty has
no encoding for that modifier, so the terminal writes the clipboard's *text* instead.

**Dragging a file typed a path instead of attaching it.** A line is treated as a drop only when every
word of it is a path that exists. One word of prose, a path naming nothing, an unterminated quote or
more than one line makes it a paste. Also: dropping a *directory* attaches nothing, and a file type
that is neither text, an image nor a PDF has its path written into the line.

**Escape does not end the session.** Use **Ctrl-C** on an empty box to leave. Escape only ever stops.
See [Interactive mode](using/interactive-mode.md#stopping-and-leaving).

**Ctrl-C did not stop anything.** It stops the *nearest* thing: with the scroller open, the first press
closes the scroller and the next one reaches the turn. The screen says which.

**Enter did nothing while a turn was running.** It queued the prompt. A running turn refuses sending
and nothing else; the queued prompt is drawn under the box until its own turn begins.

**A key press was ignored.** While the scroller is open, every key is the scroller's and a key it does
not name does nothing at all. `?` says what it takes.

## Reading and writing

**"the model cannot read this file".** The file is not covered by any trust rule, so it was
quarantined. Answer `y` at the prompt, name it with `@path`, drop it on the window, or trust the
directory. See [Trusted directories](security/trust.md).

**An edit was refused on a file the agent could clearly see.** `edit_file` requires a *trusted* file,
because locating a passage to replace is a comparison and a comparison is a decision. For a
quarantined file the route is a processor plus a write, which you approve from the diff.

**`AGENTS.md was not loaded: this directory is not trusted`**. Exactly what it says. A project's own
instructions are read through the trust map, so they load when you vouched for the directory. Your own
`~/.bravebot/AGENTS.md` is unaffected.

**A skill is not being used.** Check that `SKILL.md` has both `name` and `description` in front
matter, since a file missing either is skipped. Then check the description: it is the only part the
planner sees before loading, so it should say *when* to use the skill.

**`/commit-style` did not run my skill.** Skills are not slash commands here. Say what you want and
the planner loads the skill when the description matches. See
[Skills](customize/skills.md#skills-are-not-slash-commands).

**A write asked for approval on a file in a directory I trusted.** Untrusted data going into a trusted
path asks, because approving it also marks that path untrusted. That is the round trip being closed.
See [what a write does](security/trust.md#what-a-write-does).

## Running programs

**The agent said it could not tell whether a command succeeded.** A run's output is quarantined by
default. It has to ask you for the output with `read_output`, or you have to press `a` at the run
prompt.

**A run prompt appeared for a command I already approved.** Entries are keyed by resolved path and
exact arguments: `git log` says nothing about `git log --all`. And a run with *private* input asks
every time, whatever is vouched for.

**A command line was refused rather than run.** `$(...)`, `$VAR`, `$((...))`, `&`, here-documents and
control flow are refused, because what they stand for is not in the line and so could not be shown to
you at the prompt. Pipes, `&&`, redirection, globs and braces are all fine. The refusal names the part
of the line that caused it, and quoting turns any of them into an ordinary argument. To use a real
shell, type `!` yourself. See [`run`](reference/tools.md#run) and
[Shell mode](using/shell-mode.md).

## Long sessions

**The conversation was summarised unexpectedly.** It passed the context budget, which is the window
your model advertises. Override it with `BRAVEBOT_CONTEXT_BUDGET`, which outranks the advertised
figure. A model that advertises nothing, and the automatic entry, fall back to 24,000 prompt tokens.

**It stopped calling tools and just answered.** A bounded turn reached its round limit: the planner
is told it has no tools left, so it answers with what it has. Ask again with a narrower task. An
interactive turn carries no such limit, so this is a one-shot or manifest run, where the default is
200 rounds.

## Sessions

**`--resume` cannot find my session.** Sessions belong to the directory they ran in. Resume from the
same working directory, or pass the id printed when the session ended. A session that moved with
[`/cd`](reference/commands.md#cd-path) is recorded where it moved to, and the line printed on the way
out names that directory. Resume it from there, or use the same id in the directory you started in to
find the session as it was before the move.

**A resumed session asked a question I already answered.** Answers to the planner's own questions live
only in the running session. Standing permissions (the trust map and vouched-for commands) do come
back.

**A resumed session says it was recorded by a different build.** It is telling you the transcript is
being read against code that has moved since.

## The state directory

**`no state directory: … names nothing`**. Nothing resolved a directory to keep things in, so this
run has no settings of your own, no session to resume, no prompt history and no skills. `doctor`
names the variables it looked at and what is not kept without one. On Windows `USERPROFILE` answers
where `HOME` is unset, so a stock install resolves one.

**A session is not where I expected it.** `doctor` prints the state directory it resolved and the
variable it came from, which is the quickest way to settle where anything is being written.

## Windows

**Ctrl-G says no editor was found when one is installed.** A bare name like `notepad` or `code` has
to be found along the `PATH`, and the four terminal editors are tried there too, so Git for Windows'
vim is found. Set `$VISUAL` or `$EDITOR` to name one outright. If nothing is found on a machine that
clearly has an editor, update: an older version looked in fewer places.

**`/add-dir` and `/cd` refuse the path I gave them.** Every trust rule is keyed under a name spelled
with `/`, and a path spelled from a drive letter is not one, so on Windows both commands refuse. That
is the closed direction of the two: admitting such a directory would key its rule under a name read
as a path inside the project, where the answer you gave about the project at startup covers every file
in it. See [Trusted directories](security/trust.md).

## Still stuck

Bugs and questions go to
[the issue tracker](https://github.com/brave/bravebot/issues). The
[mini-specs](https://github.com/brave/bravebot/tree/main/docs/specs) state each
behaviour as a numbered clause and name the tests that pin it, so they are usually the fastest way to
find out whether something is intended.
