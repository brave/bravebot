# Getting started

Installing bravebot, running it, and what it asks you before and during work on a repository.

## Install

```sh
npm install -g @brave/bravebot
```

This downloads the release binary for your platform and verifies its checksum. macOS, Linux,
and Windows on both x86_64 and arm64 are supported.

On macOS and Linux there is an install script, for a machine with no npm on it:

```sh
curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh
```

It fetches the newest release for your platform, checks it against the published checksum, and
puts it in `/usr/local/bin`, asking for sudo only if that directory is not yours to write to.
`INSTALL_DIR` puts it somewhere else:

```sh
curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | INSTALL_DIR="$HOME/.local/bin" sh
```

Running the line again is how an install made this way is updated: it remembers where the last
one went, so a second run replaces that binary rather than leaving two on your PATH.

To build from source instead, see [development/README.md](development/README.md).

## Staying current

A session started on a version that has been superseded says so, once, under the trust question,
and gives the line that updates the copy you are running: the npm command for an npm install and
the script again for a script install. A build from source is left alone, since neither line would
update one.

Nothing about that waits: the notice comes from an answer written down on an earlier launch, and
the request that refreshes it, at most one a day, runs behind the session and is for the next one.
A machine that is offline, or a first run with nothing recorded yet, says nothing. An incognito
session neither records an answer nor asks for one, and a build from source never asks at all.

## Using it

```sh
bravebot                                  # interactive session
bravebot "what does src/main.rs do?"      # one-shot
bravebot "explain this" --file notes.md   # with named context
bravebot doctor                           # check configuration and confinement
```

The prompt has the editing keys you would expect, Shift-Enter (or Ctrl-J) for a new line without
sending, Ctrl-V for pasting including screenshots, Ctrl-G to compose in `$EDITOR`, and Ctrl-T for
the audit trail. Long pastes and dropped files fold to a marker you can delete. See
[specs/terminal-input.md](specs/terminal-input.md) for the full set and the terminal quirks behind a
couple of them.

Type `!` on an empty prompt and the line becomes a command for your own shell, so `! cargo test`
runs in `$SHELL` with globs and redirection intact. What it prints goes to the model in full, which
is what lets you follow it with "fix the first failure". See
[specs/shell-mode.md](specs/shell-mode.md) for what that means for trust.

## Slash commands

A line that is exactly a word beginning with `/` is acted on here rather than sent as a prompt.
`/model` chooses which model to think with. `/theme` opens a centred panel over the session and
live-previews as you move; Enter keeps the choice, Escape keeps what you had. `/theme nord`
applies a named theme without opening the panel. The choice is stored in `~/.bravebot` and applies
in every directory.

Schemes published for both a light and a dark terminal are one row, such as `gruvbox`, and follow
whichever background is sensed at startup. Where that guess is wrong, name the half you want:
`/theme gruvbox-dark` and `/theme gruvbox-light` stay put. Custom themes are JSON files under
`~/.bravebot/themes/`, and an ink there may be a pair, `{"dark": "#282828", "light": "#fbf1c7"}`,
to follow the terminal the same way. See
[specs/commands.md](specs/commands.md) for what makes a line a command, and
[specs/terminal-transcript.md](specs/terminal-transcript.md) for how themes paint the interface.

Add `--trace` to a one-shot run for the audit trail: which gate checked what, the label every
value carried, and what was released.

## Language

bravebot reads the interface in your language where a translation for it has shipped, and in
English otherwise. It takes the first of `BRAVEBOT_LOCALE`, `LC_ALL`, `LC_MESSAGES` and `LANG`
that is set, so on a machine already set up for French there is nothing to do.

```sh
bravebot                          # whatever your shell says
BRAVEBOT_LOCALE=fr bravebot       # this once
export BRAVEBOT_LOCALE=fr         # from now on
```

`BRAVEBOT_LOCALE` is there so one program can be in a language the rest of the shell is not,
which is usually wanted the other way round: an English interface on an otherwise French machine.

`fr-CA` and `fr-BE` are answered by the French catalog where they have none of their own, and a
language nothing has shipped for reads in English. `LC_ALL=C` asks for no translation at all.

English and French are what ship today. Adding a language is a file, and needs no Rust:
[crates/i18n/locales/README.md](https://github.com/brave/bravebot/blob/main/crates/i18n/locales/README.md).

What stays in English whatever you set: the names of the slash commands, so `/model` is `/model`
everywhere; the letters a question is answered with, `y` and `n`; and the audit trail, which is a
record rather than prose. Nothing the model is sent changes with your language either, so
switching it changes what you read and never what the agent does.

## Trusted directories

At startup you are asked whether you trust the working directory. **Trust it** and ordinary work
proceeds without a prompt for every edit. **Decline** and nothing is trusted, so every write is
shown to you first.

The rules, and every other way a path comes to be trusted, are in
[specs/trust-map.md](specs/trust-map.md).

## Skills and AGENTS.md

Put standing instructions in `AGENTS.md` and they apply to every task in that directory. Put a
skill in `~/.bravebot/skills/<name>/SKILL.md` and it is available in every project:

```markdown
---
name: commit-style
description: How commit messages are written here. Use before writing one.
---

Write the subject in the imperative. Explain why in the body, never what.
```

Only the name and the description are put in front of the model, which loads the body when the
task calls for it. Your own `~/.bravebot` is trusted for being yours; a project's `AGENTS.md`
and `.bravebot/skills` are read through the trust map, so they load when you vouched for the
directory and are left out when you did not. See [specs/skills.md](specs/skills.md).

## Choosing a model service

There is one thing to set up before the first session, and it is which service answers. A released
binary arrives with none configured, so rather than open a session with nothing set up to answer it,
a first run says what to configure and stops.

Configure one of these three. `bravebot doctor` reports what it will use once you have.

Each block below sets a top-level `model` key as well. A block on its own leaves the model in force
the one the build came with, which is Brave's own, so the first run says the same thing again and
names the key to set. `/model` in a session picks from every model any configured service offers,
and remembers what you picked.

### AWS Bedrock, through your own account

Put a `provider` block named `amazon-bedrock` in `~/.bravebot/settings.json`, with the region and
the models to offer. Models are keyed by the name Bedrock knows them by, which may be an
inference-profile ARN:

```json
{
  "provider": {
    "amazon-bedrock": {
      "options": { "region": "us-west-2", "profile": "my-bedrock-sso" },
      "models": {
        "arn:aws:bedrock:us-west-2:123456789012:application-inference-profile/abcdef": {
          "name": "Claude Opus (Bedrock)"
        }
      }
    }
  },
  "model": "arn:aws:bedrock:us-west-2:123456789012:application-inference-profile/abcdef"
}
```

The credential is the AWS chain rather than a token, so nothing here names one. `profile` is
optional and names a credential profile to sign with; without it the ambient credentials are used.
A profile configured for SSO is signed in to before work starts, and the sign-in prints a URL and a
code to type into it.

### OpenRouter, or another OpenAI-compatible gateway

Put a `provider` block named for the gateway, with the environment variable that holds its API key
and the models to offer:

```json
{
  "provider": {
    "openrouter": {
      "env": ["OPENROUTER_API_KEY"],
      "models": { "z-ai/glm-4.6": {} }
    }
  },
  "model": "openrouter/z-ai/glm-4.6"
}
```

`openrouter` needs no endpoint written down, its own being compiled in. Any other gateway names one
in `options.baseURL`, and a gateway naming no models at all is asked what it serves. The block is
read in the shape another tool already reads, so one copied out of that tool's configuration works
here unedited.

### Brave Leo Premium, if you already subscribe

```sh
bravebot import-leo-creds        # or `import-leo-creds development` for a development channel
```

This registers as an additional device rather than taking the browser's credentials, so Brave keeps
its own and nothing it holds is spent. Run it on a machine where Brave is signed in to the
subscription.

These models are reached through Brave's AI gateway, which has problems of its own still being
worked on, so prefer one of the two above for now. It is the shortest route if you already
subscribe.

### Pointing it somewhere else entirely

An endpoint of your own is configuration too, and a build pointed at one is not asked to configure
anything further. See [development/configuration.md](development/configuration.md).

## On a corporate network

Two things a managed network changes, both stated in the environment and both reported by
`bravebot doctor` under `network`.

**A certificate authority of your own.** A network that inspects TLS presents its own certificate,
signed by an authority your machine has been given and the released binary has not. Name it the way
you name it for every other client on the machine:

```sh
export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt   # a bundle
export SSL_CERT_DIR=/etc/ssl/certs                        # or a directory of them
```

What you name replaces the authorities built into the binary rather than adding to them, so name a
bundle that holds the public roots as well as yours. Without this, every request fails with
`invalid peer certificate: UnknownIssuer`.

**A proxy.** `ALL_PROXY`, `HTTPS_PROXY` and `HTTP_PROXY` are honoured, in upper case or lower, and
`NO_PROXY` names the hosts that bypass one. A SOCKS proxy is not supported; `doctor` says so rather
than leaving requests to go direct unannounced. A proxy that inspects TLS reads the bodies of the requests that go through
it, which for this program means the conversation; it can do that only with an authority you also
gave the machine above.

```sh
export HTTPS_PROXY=http://proxy.example.internal:8080
export NO_PROXY=localhost,127.0.0.1,.example.internal
```

`bravebot doctor` names the proxy by host and port, never its credential.
