---
id: SERVERS
title: Declaring an MCP server
status: normative
governs:
  - crates/mcp/src/lib.rs
  - crates/mcp/src/protocol.rs
  - crates/mcp/src/stdio.rs
  - crates/mcp/src/http.rs
  - crates/config/src/mcp.rs
  - crates/config/src/managed.rs
  - crates/config/src/settings.rs
  - crates/config/src/import.rs
  - crates/cli/src/mcp.rs
  - crates/cli/src/servers.rs
  - crates/core/src/capability.rs
  - crates/core/src/permissions.rs
  - crates/core/src/policy.rs
  - crates/agent/src/tools.rs
  - crates/agent/src/mcp.rs
  - crates/agent/src/permission_mode.rs
  - crates/cli/src/plain.rs
  - crates/tui/src/confirm.rs
  - crates/tui/src/status.rs
documented-by:
  - docs/website/docs/customize/mcp-servers.md
  - docs/website/docs/customize/mcp/gmail.md
  - docs/website/docs/customize/mcp/calendar.md
  - docs/website/docs/customize/mcp/github.md
---

## Scope

How a person says that a server exists, what that statement is trusted for, where the statement is
kept, and what is asked before a tool from one runs. The protocol itself, what a result is labelled
and why a stdio server is confined, is [mcp.md](mcp.md) and is unchanged here. This spec is the
surface that spec has none of.

## What exists today

Declaring a server is built, and so are starting one, offering its tools and calling one.
`bravebot mcp` writes `~/.bravebot/mcp.json`, asks the question that approves one, records the digest
it was asked about, writes and takes out the request in the settings file `-s` names, lists what is
declared, and forgets a project's standing answers ([SERVERS-1](#SERVERS-1),
[SERVERS-2](#SERVERS-2), [SERVERS-3](#SERVERS-3), [SERVERS-5](#SERVERS-5), and the `list` half of
[SERVERS-14](#SERVERS-14)). `doctor` names a settings layer that tries to declare one.

The machine's managed layer can keep a server from starting, by the host its url names or the
command it runs, and can add none. An allow list starts only what it names, a deny entry wins over
it, and neither names an alias. A session starts no server the layer refuses, in any mode, and says
which file refused it and why; `list` and `get` say the same beside the approval, and `doctor`
names each list among what the layer pins ([SERVERS-12](#SERVERS-12)).

A session reads the aliases its checkout requests ([SERVERS-2](#SERVERS-2)), resolves each against
those declarations, and puts the three-answer question to the person where no answer of theirs
covers it, naming a runner and an unpinned package as it does ([SERVERS-4](#SERVERS-4),
[SERVERS-6](#SERVERS-6)). An approved stdio server is started confined, holding the variables it
names and no others, and an HTTP one is reached through the egress gate, where a hop off its
declaration is put to the person rather than followed ([SERVERS-10](#SERVERS-10),
[SERVERS-11](#SERVERS-11)). Each server started completes its
handshake and is held, with a grant naming it, for as long as the session runs
([SERVERS-9](#SERVERS-9)). The session's display names what it started, and how
each one's tools stand ([SERVERS-14](#SERVERS-14)). `bravebot-cli` starts a server and
`bravebot-agent` offers its tools and calls them, and both depend on `crates/mcp/`.

At the first turn a person asks for, each started server's list of tools is put to them as the
client drew it, every description whole and behind the margin, and nothing of the list reaches the
planner until they say yes ([SERVERS-8](#SERVERS-8)). A yes offers each tool under the alias and
records a digest of the list, so the same list is offered unasked in a later session and a changed
one is asked about again. Each call is then put to them with three answers, after any permission
rule naming the server or the tool ([SERVERS-7](#SERVERS-7)). Bypassing answers both questions and
records neither ([SERVERS-13](#SERVERS-13)).

The desktop application starts no server, and says so as a session opens where one is requested
([SERVERS-2](#SERVERS-2)). That is a cost it pays on purpose, and the known costs at the end give the
reason.

Issue #83 is where the unwired client was written down, and it names the four things wiring needs
decided first: where a server is declared, what a name and an argv is trusted for, how the untrusted
label [mcp.md](mcp.md) puts on a result reaches a slot, and whether servers are confined. Every
clause below answers one of them. Whether a server's description reaches the planner at all was
settled there too: the person vouches for the tool list after the handshake, and its digest is
recorded beside the server's approval.

Every clause names the tests that pin it. Where a built clause has an unbuilt half, the clause says
which, and that sentence is what keeps a reader from taking the present tense as a claim about the
current build.

## The parity target

What Claude Code offers, as the list this spec is measured against. Three rows in bold are where
parity is refused on purpose. One row in italics is where the behaviour is adopted and the storage
is not, and [the divergence](#the-one-place-this-diverges-from-claude-code) says why.

| | Claude Code | This spec |
|---|---|---|
| Declaration file | `.mcp.json` at the repo root, or user scope | `~/.bravebot/mcp.json`, the person's own directory only |
| A file in the checkout may declare a server | yes, argv included | **no**, an alias may be requested and nothing more |
| Scopes | local, project, user | home declares; a checkout or the home layer requests, in the file `-s` names; a machine-level layer removes |
| Prompt when a server is first reachable | three answers: use it, use it and all future servers in this project, continue without it | same three answers, same meanings |
| Prompt on every tool call | three answers: yes, yes and stop asking for this tool here, no | same three answers, same meanings |
| *Where "stop asking" is recorded* | *`.claude/settings.local.json`, inside the checkout* | *the state directory, keyed by the project path* |
| Adding one from the command line | `claude mcp add [-s <scope>]` | `bravebot mcp add [-s <scope>]`, and the typing is not the approval; the scope says where the request goes, and the declaration is in one place whatever it says |
| Re-approval when the declaration changes | not on argv change | **yes**, an approval binds to a digest |
| Environment reaching the server | the whole process environment, plus `env` | the named variables and the values `env` stores, and nothing else |
| A value given at `add` | `-e NAME=value`, stored in the declaration | the same, stored in `~/.bravebot/mcp.json`, and shown by its name and never as itself, or as the file it may read where it names one |
| Unpinned command (`npx -y pkg@latest`) | run as written, silently | named at the prompt as code that differs per run |
| Tool names | as the server reports them | namespaced by the local alias; a server's own words are never an identifier |
| Tool descriptions | into the planner's context as trusted text | untrusted until a person vouches for the list they are on, and marked |
| Confinement of a stdio server | none | required already by [MCP-3](mcp.md#MCP-3) |
| A remote server | fetched directly | an egress destination, and the host is approved |
| A mode that skips the prompts | `--dangerously-skip-permissions` | [SERVERS-13](#SERVERS-13), and it skips prompts only |
| An administrator's server lists | `allowedMcpServers` and `deniedMcpServers`, by name, command or url; a managed file may also add servers | `mcp.allow` and `mcp.deny`, by host or command; **no** name form, and the managed layer adds nothing |

## Why a settings layer cannot declare a server

[BACKEND-1](backends.md#BACKEND-1) is the obstacle, and it is right. A settings file may name which
region, which profile, which host, which model, and what to add to a request. Then: "Nothing in a
settings file grants a capability or vouches for a path. None of them names a command to run." A
stdio server declaration is a command to run. The clause's own reasoning says why that is not a
detail to amend around: these files are read before anything runs and are the easiest thing on the
machine to write to, so a capability granted from one is a capability granted by whatever last
edited it.

There is a second reason, and it is the one that decides the shape rather than merely forbidding the
easy road. **The agent can write files inside the workspace.** That is its job. A file in the
workspace that could declare an argv would therefore be a path from an ordinary tool call to
arbitrary execution, and the write that took it would look like any other edit.
`.bravebot/settings.local.json` is inside the workspace whatever an ignore file says about it, so
being uncommitted buys nothing here: the threat is not a teammate's checkout, it is this program's
own write gate.

So a declaration carrying an argv lives in the person's own directory and nowhere a session can
reach. What a checkout keeps is the half that is useful and grants nothing: the alias of a server it
expects, which resolves against what the person already declared, or resolves to nothing.

## The one place this diverges from Claude Code

The gates below are Claude Code's, deliberately: the same three answers at the same two moments,
with the same meanings, because that shape is well understood and a person moving between the two
tools should not have to learn a second one.

What is not adopted is **where the standing answer is written.** Claude Code records
`enabledMcpjsonServers` in `.claude/settings.local.json`, inside the checkout. Here the equivalent
record goes in the state directory, keyed by the project's path.

The reason is the confused deputy. A write into the workspace is an effect this agent can ask for,
and writes are gated one at a time. A person approving "write `.bravebot/settings.local.json`" is
answering a question about a file, and if that file's contents are a grant then the answer they gave
was to a different question than the one that mattered. Where a permission rule already allows
writes under the workspace, which is an ordinary thing to configure, there is no question at all.
Recording it outside every directory a session can write closes that without changing a single thing
a person sees.

The cost is real and worth stating: the record no longer travels with the checkout, so a second
machine asks again, and a deliberate `rm -rf` of the workspace no longer forgets the answer.
`bravebot mcp forget <path>` is how a person drops it instead.

## The command line

```
bravebot mcp add <alias> [-s <scope>] [-e|--env <name>[=<value>]...]... [--dir <path>] [--stdio] -- <program> [args...]
bravebot mcp add <alias> [-s <scope>] --http <url>
bravebot mcp get <alias>
bravebot mcp list
bravebot mcp approve <alias>
bravebot mcp enable <alias> [-s <scope>]
bravebot mcp disable <alias> [-s <scope>]
bravebot mcp remove <alias>
bravebot mcp forget [path]
```

`add` writes a declaration and then asks, and a yes also requests the server. `approve` is the same
question for a declaration that is already written, which is what a checkout's request and a
changed digest both produce. `enable` requests a declared server, asking first where it is not
approved, and `disable` takes the request out ([SERVERS-2](#SERVERS-2)). `-s` names the file a
request goes in, as Claude Code's `-s` names where its declaration goes: `local`, the default, is
`.bravebot/settings.local.json` in the current directory, `project` is `.bravebot/settings.json`
there, and `user` is `~/.bravebot/settings.json`, which every session reads. `get` shows
one declaration in full, including its digest and the variables it will receive. `list` says, per
alias, the transport, whether it is approved and the digest, which file here requested it, whether a
session started here holds a grant to call it, and what is answered about it here; `doctor` says
the same ([SERVERS-14](#SERVERS-14)). `remove` deletes the declaration and its approval together,
because an approval outliving its declaration is a digest nothing resolves. `forget` drops the
standing answers recorded for a project, defaulting to this one.

A bare `--` takes the program and its arguments after it, as argv and never as a line, and every
word after it is the server's, a flag of bravebot's included. `--stdio --` is the same and may be
written instead. Nothing here compiles shell syntax: that is
[tools/command-line.md](tools/command-line.md)'s road, for a line a planner wrote, and a server is a
program a person named. `-e` or `--env` gives the server a variable
([SERVERS-10](#SERVERS-10)): `NAME=value` stores the value, and a name alone is read from the
environment at launch. It takes the words up to the next flag, as Claude Code's does, and may be
given more than once. A program given by a bare name has `PATH` named for it where none was stored;
`--dir <path>` is the directory it runs in, written into the file as the absolute path it resolves
to. A directory git would open a repository from is refused, as [SERVERS-10](#SERVERS-10) says.

`forget` takes one path at most, and the current directory without one. The path is read with its
links followed, as answer 2 recorded it, and one that no longer resolves, a deleted checkout's, is
taken as typed and made absolute. It drops that project from `mcp-projects` and every standing
answer about a tool in it from `mcp-tools`, says a line for each answer it dropped or that nothing
was recorded, and leaves every other project's as it was. An incognito session writes nothing, so
`forget` is refused there.

### Coming from Claude Code

| Claude Code | Here | Note |
|---|---|---|
| `claude mcp add weather -- npx -y @dangahagan/weather-mcp@latest` | `bravebot mcp add weather -- npx -y @dangahagan/weather-mcp@latest` | The same line. It declares `PATH` for `npx`, and the question shows it. |
| `claude mcp add my-server -e API_KEY=xxx -- npx my-mcp-server` | the same line | The value is stored in `~/.bravebot/mcp.json` and shown as `API_KEY (stored)`, never as itself. |
| `claude mcp add brave-search -e BRAVE_API_KEY_FILE=/path/to/key -- npx -y @brave/brave-search-mcp-server` | the same line | The value is stored in `~/.bravebot/mcp.json`. The key file it names is one the server may read, and the question shows it as `may read`. |
| `claude mcp add gmail -e WORKSPACE_FEATURE_OVERRIDES=... -- node ~/google-workspace-mcp/dist/index.js` | `bravebot mcp add gmail -e GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE=true -e WORKSPACE_FEATURE_OVERRIDES=... -e BROWSER=www-browser --dir ~/google-workspace-mcp -- node ~/google-workspace-mcp/dist/index.js` | Google's Workspace server. Claude Code runs it with the person's `HOME`. Here `HOME` is the server's own ([SERVERS-10](#SERVERS-10)), so the server keeps its token in a file in its own directory, and `--dir` makes that directory writable. The sign-in, `node dist/headless-login.js`, is run first and outside bravebot. What the server returns is private ([MCP-1](mcp.md#MCP-1)). |
| `claude mcp add calendar -e WORKSPACE_FEATURE_OVERRIDES=... -- node ~/google-workspace-calendar-mcp/dist/index.js` | `bravebot mcp add calendar -e GEMINI_CLI_WORKSPACE_FORCE_FILE_STORAGE=true -e WORKSPACE_FEATURE_OVERRIDES=... -e BROWSER=www-browser --dir ~/google-workspace-calendar-mcp -- node ~/google-workspace-calendar-mcp/dist/index.js` | The same server as the Gmail row, with the Calendar read group left on and every other group off, in a directory of its own so each token has one. The sign-in, `node dist/headless-login.js`, is run first and outside bravebot. What the server returns is private ([MCP-1](mcp.md#MCP-1)). |
| `claude mcp add github -e GITHUB_PERSONAL_ACCESS_TOKEN=... -- docker run -i --rm -e GITHUB_PERSONAL_ACCESS_TOKEN ghcr.io/github/github-mcp-server` | `bravebot mcp add github -e GITHUB_PERSONAL_ACCESS_TOKEN=... -- ~/github-mcp-server/github-mcp-server stdio --read-only --lockdown-mode --toolsets issues,pull_requests` | GitHub's own server, run as its released binary. The Docker line does not work here, because a confined server does not reach the Docker daemon, and the hosted server needs an `Authorization` header that `--http` does not send. The flags are part of the declaration, so they show at the question. What the server returns is private ([MCP-1](mcp.md#MCP-1)). |
| `claude mcp add weather -s user -- ...` | `bravebot mcp add weather -s user -- ...` | The declaration goes in `~/.bravebot/mcp.json` whatever the scope. The request goes in `~/.bravebot/settings.json`, so every session starts it. |
| `claude mcp add weather -s project -- ...` | `bravebot mcp add weather -s project -- ...` | [SERVERS-1](#SERVERS-1). The checkout's `.bravebot/settings.json` gets the alias and not the argv. |
| `claude mcp get weather` | `bravebot mcp get weather` | Also prints the digest, which is what an approval is against. |
| `claude mcp remove weather -s project` | `bravebot mcp disable weather -s project` | Takes the alias out of `.bravebot/settings.json` and leaves the declaration, which is in no checkout to remove. |
| `claude mcp list` | `bravebot mcp list` | Unapproved declarations are listed as unapproved rather than omitted. |

## Where things are stored

| What | Where | Scope | Lifetime |
|---|---|---|---|
| A declaration: alias, transport, argv or url, the variable names it needs, the values it stores, the files it may read, its directory | `~/.bravebot/mcp.json` | the person's own directory | until they change it |
| An approval of a server, one digest and the alias it was given about per line, and a `changed` line per alias whose approved declaration changed since | `~/.bravebot/mcp-approved` | the person's own directory | until the declaration changes |
| The list of tools a person vouched for, a `tools` line holding the declaration's digest and the list's | `~/.bravebot/mcp-approved` | the person's own directory | until another list is vouched for under that declaration, or no declaration resolves to it |
| "use all future servers in this project", one project path per line | `~/.bravebot/mcp-projects` | the person's own directory | until `mcp forget` |
| "stop asking for this tool here", one alias, tool and project path per line | `~/.bravebot/mcp-tools` | the person's own directory | until `mcp forget` |
| What a local server writes in its own home, such as a runner's cache ([SERVERS-10](#SERVERS-10)) | `~/.bravebot/mcp-home/<digest>` | one declaration | until somebody removes it |
| A request for an alias, `"mcp": { "request": ["weather"] }` | `.bravebot/settings.json` or `.bravebot/settings.local.json` beside the work, or `~/.bravebot/settings.json` | that checkout, or every session | until `mcp disable` or an edit takes it out |
| What may start, `"mcp": { "allow": [{ "host": "*.corp.example" }], "deny": [{ "command": ["/opt/weather-mcp"] }] }` | the managed layer | the machine | as long as it is pinned |
| A declaration, an argv, a url, or a variable's value | `.bravebot/settings.json` or `.bravebot/settings.local.json` | **nothing.** [SERVERS-1](#SERVERS-1) | n/a |
| A variable's value given at `add` as `-e NAME=value` | `~/.bravebot/mcp.json`, under `env` | the person's own directory | until they change it |
| A variable's value given by name | nowhere. The value is the person's own environment at launch | n/a | n/a |

`~/.bravebot` is the state directory, which on Unix is readable only by the user
([STATE-1](state-directory.md#STATE-1)), and a machine naming no profile directory has no state
directory rather than a guessed one ([STATE-2](state-directory.md#STATE-2)). A machine with no state
directory declares no servers and records no answers, so every prompt is asked afresh and none can
be stored, which is the same answer absence gives everywhere else here.

The shape of these records mirrors [vetting.md](vetting.md)'s: a word in a file in the person's own
directory for the standing answer, and the key that changes behaviour read from the home layer and
no other.

## Clauses

<a id="SERVERS-1"></a>
### SERVERS-1: a declaration lives in the person's own directory, and no file inside a checkout makes one

The file that may carry an alias, a transport, an argv, a url, the names of the variables a server
needs, the values it stores for others, the files it may read, and the directory it runs in is
`~/.bravebot/mcp.json`. A settings layer carries none of
those, at any of the three levels, including the one the command line names.

A project layer that names an argv is a parse error reported by `doctor`, not a declaration that is
quietly ignored, because a person who wrote one believes it works.

**Why.** [BACKEND-1](backends.md#BACKEND-1) forbids a settings file from naming a command to run,
and the agent's own write gate covers the workspace, so any in-checkout file that could name one
turns a tool call into execution nobody approved. Keeping the declaration outside every layer also
keeps `doctor`'s answer about settings free of a question it cannot settle.

What a settings layer is taken to be declaring is `mcpServers`, the key another tool declares with,
any key under `mcp` other than `request`, which a checkout's layer carries, and `allow` and `deny`,
which only the managed layer acts on ([SERVERS-12](#SERVERS-12)), and an `mcp` that is not an
object at all.
Each one is a `doctor` line naming the key and the file, and the report fails on it. The line names nothing
inside the entry, since the entry may hold an argv and the values of variables, and the rest of the
file is read as it would have been.

`verified-by: bravebot_config::settings::every_layer_that_names_a_server_is_recorded_and_none_declares_one`
`verified-by: bravebot_config::settings::a_request_or_a_server_list_in_the_mcp_block_is_not_a_declaration`
`verified-by: bravebot_config::mcp::the_files_are_read_from_the_state_directory`
`verified-by: bravebot_cli::running::doctor_reports_a_server_declared_in_a_checkouts_settings`

<a id="SERVERS-2"></a>
### SERVERS-2: a checkout requests an alias and grants nothing

`.bravebot/settings.json` may carry `"mcp": { "request": ["weather"] }`, a list of names. A name
resolves against the declarations in the person's own directory. One that resolves to nothing is
reported to the person as a server this checkout expects and they have not declared, and nothing is
fetched, installed, or run to satisfy it.

A request is not an approval. An alias that resolves to a declaration the person has not approved
asks, per [SERVERS-4](#SERVERS-4).

**Why.** This is the whole of what a shared file can safely say. "This project uses a weather
server" is information; "this project runs `npx -y whatever@latest`" is an instruction, and the
difference is exactly the line [BACKEND-1](backends.md#BACKEND-1) draws. It also makes the common
case work: a checkout can tell a newcomer what it wants without being able to hand them anything.

Every layer's `request` is read, as [BACKEND-24](backends.md#BACKEND-24)'s rows that keep each
layer's entries are, and an alias two layers name is requested once, under the first file that named
it. An alias nobody declared is a line at the session's start naming the file and the alias, and
saying that `bravebot mcp add` declares one. The file is named relative to the checkout where it is
inside it, a checkout reached through a link included. A declared alias no checkout requested is
not started, approved or not.

The desktop application starts none of what is requested (see the known costs). A session it
starts, reopens or forks where the settings request servers names them, and says the desktop app
does not start them and that `bravebot mcp list`, run there in a terminal, says which a session
there would, so a request it passed over does not read as one it honoured. A requested name that is not an alias is shown quoted, with its escapes.

`bravebot mcp enable <alias>` writes the request into the file its scope names, the one the reader
takes that layer from: `.bravebot/settings.local.json` in the current directory where `-s` is absent
or `local`, `.bravebot/settings.json` there for `project`, and `~/.bravebot/settings.json` for
`user`. It creates the file, the `mcp` block and its list where each is missing, adds the alias
once, and leaves every other key and alias as it was. An alias that is not declared is refused, and
so is one the person does not approve at [SERVERS-3](#SERVERS-3)'s question, since a request for a
server nobody approved is one every session stops to ask about. `bravebot mcp disable <alias>`
takes the alias out of that file's list, and without `-s` out of each of the three that holds it,
naming each; where none does, it says so and names the files it read. The declaration and its
approval stay.

A file that does not parse is refused and left as it was, and `enable` refuses one whose `mcp` or
`mcp.request` the reader would not take as a block and a list, since replacing it would lose what
somebody wrote there; a `request` that is `null` holds nothing to lose, and is replaced. `disable`
reads every file before it writes any, so one it cannot read leaves the alias in all of them rather
than gone from some, and reads a file two scopes name once: run from the directory the state
directory is in, the checkout's `.bravebot/settings.json` is the user's own. In a checkout, a link at
`.bravebot` or at the file is refused and not followed: it arrived with the clone, so where it leads
is the choice of whoever wrote the checkout. A file in the state directory is written through a link,
as an import writes it, whichever scope named it. The links are looked at again just before the
write, so one made while the question waited is refused too.

`verified-by: bravebot_config::settings::every_layers_request_is_read_and_each_alias_is_kept_once`
`verified-by: bravebot_cli::servers::a_request_nobody_declared_is_reported_and_nothing_is_started_for_it`
`verified-by: bravebot_cli::servers::a_checkout_reached_through_a_link_names_its_settings_file_inside_it`
`verified-by: bravebot_session::sessions::a_desktop_session_names_the_servers_it_does_not_start`
`verified-by: bravebot_ui_bridge::servers::every_way_a_desktop_session_opens_names_the_servers_its_project_requests`
`verified-by: bravebot_cli::mcp::enable_requests_the_server_in_the_file_its_scope_names`
`verified-by: bravebot_cli::mcp::enable_adds_the_alias_once_and_keeps_the_rest_of_the_file`
`verified-by: bravebot_cli::mcp::enable_leaves_a_settings_file_it_cannot_add_to_as_it_is`
`verified-by: bravebot_cli::mcp::enable_writes_through_no_link_in_a_checkout`
`verified-by: bravebot_cli::mcp::disable_takes_the_request_out_of_each_file_that_holds_it`
`verified-by: bravebot_cli::mcp::disable_writes_no_file_where_one_cannot_be_read`
`verified-by: bravebot_cli::mcp::disable_where_the_state_directory_is_writes_its_file_once`
`verified-by: bravebot_cli::mcp::a_linked_state_directory_is_written_through_from_the_directory_it_is_in`
`verified-by: bravebot_cli::mcp::a_link_made_while_the_question_waits_is_not_written_through`
`verified-by: bravebot_config::import::a_destination_requests_a_server_once_and_withdraws_only_it`
`verified-by: bravebot_cli::running::a_server_enabled_in_a_checkout_is_requested_there_until_disabled`

<a id="SERVERS-3"></a>
### SERVERS-3: adding a server is a command a person types, and typing it is not the approval

`bravebot mcp add` writes the declaration and then asks [SERVERS-4](#SERVERS-4)'s question. The
answer to that question is what grants anything.

**Why.** A person typing a command line is the strongest endorsement this program has, and it is
still not an endorsement of what the line will do on the tenth run. Splitting the two means the
record of what was approved is a record of a thing somebody read, and it is what makes the
re-approval in [SERVERS-5](#SERVERS-5) meaningful rather than a formality.

The question `add`, `approve` and `enable` ask draws what [SERVERS-4](#SERVERS-4) shows, less the checkout
that requested it, and is answered in a line with `y` or `n`: that clause's answers 1 and 3. Answer 2
records a project path, and a command typed outside a session has no project to record, so it is
asked where a checkout's request is: at a session's start. Only the yes records
anything, and a blank line, any other word, or the end of the input is
the no. The question is put only where stdin and stdout are both a terminal; with either one piped
nobody is asked, `add` still writes the declaration and names the `enable` that asks again, and
`approve` and `enable` are refused. An incognito session writes nothing to the state directory, so
each of `add`, `approve`, `enable`, `disable` and `remove` is refused there and leaves the files as
they were.

`add` takes `-s` with `enable`'s values and default, before the alias or among the flags after it,
and requests the server in that file where the answer is yes or the digest was approved already. A
no, or nobody to ask, leaves it declared and requested nowhere, and the line names the `enable`
with the same `-s`; where that file requests the alias already, the line says so instead, since the
next session that reads it asks, and `enable` ends refused the same way. A scope is `local`,
`project` or `user`, given once; any other value, a `-s` with none, or a second `-s` is refused
before anything is written. So is a settings file [SERVERS-2](#SERVERS-2) refuses to write, or one
the request would take past the size a settings file may be, which `add` reads before the
declaration, so a refused request leaves no declaration behind it.

`verified-by: bravebot_cli::mcp::a_yes_at_the_question_records_the_digest_and_nothing_else_does`
`verified-by: bravebot_cli::mcp::approve_records_only_on_a_yes_and_a_no_ends_refused`
`verified-by: bravebot_cli::mcp::the_question_shows_every_argument_as_the_word_it_is`
`verified-by: bravebot_cli::mcp::a_bare_double_dash_declares_the_program_after_it`
`verified-by: bravebot_cli::running::an_added_server_nobody_was_asked_about_is_declared_and_listed_unapproved`
`verified-by: bravebot_cli::running::a_flag_of_bravebots_after_the_bare_dashes_is_the_servers_argument`
`verified-by: bravebot_cli::running::approving_with_nobody_to_ask_is_refused_and_records_nothing`
`verified-by: bravebot_cli::running::a_server_is_not_declared_in_an_incognito_session`
`verified-by: bravebot_cli::mcp::enable_writes_nothing_without_a_yes`
`verified-by: bravebot_cli::mcp::add_requests_the_server_in_its_scope_only_once_approved`
`verified-by: bravebot_cli::mcp::a_scope_is_one_of_three_and_given_once`
`verified-by: bravebot_cli::mcp::enable_and_disable_write_nothing_incognito`
`verified-by: bravebot_cli::mcp::add_refuses_a_request_the_file_cannot_hold_before_declaring_anything`
`verified-by: bravebot_cli::mcp::a_no_says_a_request_already_in_the_file_still_stands`

<a id="SERVERS-4"></a>
### SERVERS-4: a server reachable for the first time is put to the person, with three answers

Before any tool from a server is offered to the planner, the person is shown the alias, the
transport, the resolved program and its argv with argument boundaries visible, or the url and its
host, the variable names it will receive, each one whose value the declaration stores marked as
stored, the files it may read, the directory it will run in, the digest, and which checkout
requested it where one did. A stored value is never shown, except that one naming a file is shown
as the file it may read, with its links followed.

```
  weather   stdio   npx -y @dangahagan/weather-mcp@latest
            requested by .bravebot/settings.json
            variables: PATH
            digest: 4f1c9a2e

  Use this MCP server?
❯ 1. Yes
  2. Yes, and use all future MCP servers in this project
  3. No, continue without this server
```

Answer 1 records an approval of this digest. Answer 2 records the approval and records this project
path, so a later server in this checkout is reachable without this question. Answer 3 records
nothing and the session continues with the server absent, which is not an error and is not retried.

Answer 2 pre-answers this question and no other. It does not grant a capability, it does not
pre-answer [SERVERS-8](#SERVERS-8)'s question about a list or [SERVERS-7](#SERVERS-7)'s about a
call, and it does not reach a project other than the one it was given in.

Where nobody can be asked, the server is absent and the reason is said, which is
[LAYER-1](layering.md#LAYER-1)'s constraint on the CLI crate: where nobody can be asked, effects are
refused rather than applied unseen. A one-shot run, a run with no terminal, and a delegate all take
that road. None of them approves a server.

**Why three answers rather than two.** The middle one is the answer people actually want in a
checkout they trust, and its absence is what drives them to find a switch that turns the asking off
for everything everywhere. Bounded to one project it is a much smaller claim than that switch, and
it is the claim they meant.

The question is asked as a session opens, once per requested server no answer covers, and before
anything of the server runs. The illustration above is its shape; what is drawn is `bravebot mcp`'s
own layout, with the lines this question adds beneath the first:

```
  weather   stdio   npx -y @dangahagan/weather-mcp@latest
            requested by .bravebot/settings.json
            runs /opt/homebrew/bin/npx
            variables: PATH
            digest: 4f1c9a2e
            npx fetches what it runs when it starts
            @dangahagan/weather-mcp@latest names no exact version, so it runs whatever is published under it

  Use this MCP server?
  1. Yes
  2. Yes, and use all future MCP servers in this project
  3. No, continue without this server
  [1/2/3]
```

`runs` is the path a program named through `PATH` resolved to, and is drawn only where that differs
from what was declared. A variable whose value is stored is drawn among the others as
`BRAVE_API_KEY_FILE (stored)`, and each file the declaration may read is a line of its own,
`may read: /Users/me/keys/brave-api-key`. The directory it runs in is drawn as one it may write,
`directory, which it may write: /Users/me/weather-mcp`. Anything typed but 1 or 2, and the end of the input, is 3. The project answer
2 records is the workspace root with its links followed, and is matched as that path and no other.
Answer 3 is a line saying the server is not used in this session.

The question is put only where stdin and stdout are both a terminal, for [SERVERS-3](#SERVERS-3)'s
reason. In the full-screen interface it is asked on the plain terminal before that interface takes
the screen, and in the plain one after the question about trusting the directory. A one-shot run
asks nobody and says so on stderr, naming `bravebot mcp approve <alias>` as the way to answer at a
terminal, and a session with no terminal says the same. A delegate asks nobody, and holds a
server's grant only where the turn that spawned it holds one ([SERVERS-9](#SERVERS-9)), so it
reaches no server the session did not start. An incognito session asks, and a yes there
starts the server for that session and records nothing.

A yes whose record cannot be written still starts the server, since the person said yes and a file
that would not take the answer does not unsay it, and a line says which record was not kept.

`verified-by: bravebot_cli::servers::answer_one_approves_the_digest_answer_two_the_project_and_three_nothing`
`verified-by: bravebot_cli::servers::the_question_names_the_checkout_that_requested_it_and_where_the_program_resolved`
`verified-by: bravebot_cli::servers::nobody_to_ask_leaves_the_server_absent_says_why_and_records_nothing`
`verified-by: bravebot_cli::servers::an_approved_digest_starts_unasked_and_a_recorded_project_answers_only_an_unchanged_one`
`verified-by: bravebot_cli::servers::an_incognito_yes_is_for_this_session_and_writes_nothing`
`verified-by: bravebot_config::mcp::a_project_reads_back_as_the_path_it_was_recorded_as`
`verified-by: bravebot_config::mcp::forgetting_a_project_drops_it_and_no_other`
`verified-by: bravebot_cli::mcp::forget_drops_a_projects_standing_answers_and_nobody_elses`
`verified-by: bravebot_agent::turn::a_turn_holds_a_grant_per_server_it_was_handed_and_only_its_worker_holds_them_too`
`verified-by: bravebot_cli::mcp::a_stored_value_is_shown_by_its_name_and_never_as_itself`
`verified-by: bravebot_cli::running::a_servers_own_install_line_naming_a_key_file_declares_a_read_of_that_file`

<a id="SERVERS-5"></a>
### SERVERS-5: an approval binds to a digest of the declaration

The digest covers the transport, the program and every argument, the url and its host, the names of
the variables passed in, each value the declaration stores, each file it may read, and the
directory. It does not cover the value of a variable read from the environment, which is not in the
declaration.

Changing any of those produces a digest nothing approved, so the server is unapproved until
[SERVERS-4](#SERVERS-4)'s question is answered again, and the question names what changed. A project
path recorded by answer 2 does not answer for a changed digest: it reaches a server this person has
not seen, and a changed argv is a server they have not seen.

**Why.** Same reason [CMDLINE-3](tools/command-line.md#CMDLINE-3) binds an endorsement to the
compiled plan rather than the line: the thing a person read has to be the thing that runs, and an
approval keyed on an alias would let an edit to the argv inherit the answer given about a different
program. An alias is a label a person chose; it is not what the label points at.

The digest is SHA-256 over the declaration written as one JSON array, with the alias left out. Each
argument is its own quoted string there, so two arguments never digest as one holding the same
characters, and the variable names are a sorted set, since their order changes nothing a server
receives. The stored values and the files it may read follow as one object, and only where there
is either, so a declaration storing neither digests as it did before a value could be stored and
its approval still stands. `mcp-approved` holds one
digest per line in hex, followed by the alias it was approved about. `add` replacing a declaration
names the fields that changed, a changed value by its variable's name and never by either value,
and a digest no declaration resolves to any longer is dropped whenever `add`, `approve` or `remove`
rewrites the file.

The alias beside a digest approves nothing: whether a declaration is approved is asked of its digest
alone. It is kept so a session can tell a server nobody was asked about from one that changed since
somebody answered, which a recorded project may pre-answer and this may not. So a dropped digest
leaves a `changed <alias>` line behind, the question at a session's start says the declaration
changed since it was approved, and a yes clears the line. A digest alone on its line, as the file
was written before the alias was kept, is still an approval, and takes its alias when it is next
rewritten. The session's question says *that* the declaration changed and not which fields: only a
digest is kept of what was approved, and a digest names no field.

`verified-by: bravebot_config::mcp::a_digest_covers_the_program_every_argument_the_names_and_the_directory`
`verified-by: bravebot_config::mcp::a_digest_covers_every_stored_value_and_every_file_it_may_read`
`verified-by: bravebot_config::mcp::a_declaration_storing_no_value_digests_as_it_did_before_values_could_be_stored`
`verified-by: bravebot_config::mcp::two_arguments_digest_apart_from_one_holding_the_same_characters`
`verified-by: bravebot_config::mcp::a_digest_is_the_same_whatever_the_alias_or_the_order_of_the_names`
`verified-by: bravebot_config::mcp::what_changed_is_named_by_field`
`verified-by: bravebot_config::mcp::a_digest_reads_back_from_the_line_it_is_written_as`
`verified-by: bravebot_config::mcp::an_approval_nothing_resolves_to_is_dropped`
`verified-by: bravebot_cli::mcp::replacing_a_declaration_names_what_changed_and_asks_again`
`verified-by: bravebot_cli::running::get_shows_the_digest_an_approval_binds_to`
`verified-by: bravebot_cli::running::removing_a_server_drops_its_approval_and_no_other`
`verified-by: bravebot_config::mcp::an_approval_reads_back_with_the_alias_it_was_given_about`
`verified-by: bravebot_config::mcp::a_declaration_changed_since_its_approval_is_recorded_as_changed_and_approves_nothing`
`verified-by: bravebot_config::mcp::a_digest_alone_on_its_line_takes_its_alias_when_rewritten`
`verified-by: bravebot_cli::mcp::replacing_a_declaration_approved_under_no_alias_still_says_it_changed`
`verified-by: bravebot_cli::servers::an_approved_digest_starts_unasked_and_a_recorded_project_answers_only_an_unchanged_one`

<a id="SERVERS-6"></a>
### SERVERS-6: a command that fetches its own code is named as one at the prompt

Where the program is a runner that resolves and executes a package at launch, `npx`, `uvx`, `pipx`
and their like, the prompt says so, and says that what runs is decided when it runs rather than by
the argv on the screen. An unpinned version is named as unpinned.

The approval still binds to the digest of the argv, which is all a digest can cover. This clause
buys no guarantee. It makes the one thing a digest cannot promise visible at the moment somebody is
deciding.

**Why.** `npx -y pkg@latest` is the form nearly every server is distributed as, and it is a
different program on different days. Approving it once is approving a channel, not code, and a
person who is not told that reasonably believes the digest means more than it does. Refusing the
form outright would refuse the whole ecosystem; saying nothing would launder a supply chain through
an approval prompt.

The runners are `npx`, `bunx`, `npm exec`, `pnpm dlx` and `yarn dlx` for Node, and `uvx`,
`uv tool run` and `pipx run` for Python, recognised by the name of the declared program whatever
directory it is given in. The package is the one a `--package` or `-p` flag names for Node, or
`--from` or `--spec` for Python, and otherwise the first word that is not a flag or the value of a
flag known to take one, such as `--registry` or `--with`. A flag the reading does not know may take
the next word, so where one comes before the package the prompt says which package runs is not
known, rather than name a word that may not be it. A Node package is pinned where the version after
its last `@` is one exact release, `@scope/name@1.4.2`, and a Python one where `==` or `@` is
followed by one; a tag such as `latest`, a range, and no version at all are each unpinned. An exact
release is numbers joined by dots, with a pre-release or build suffix allowed after them, and for
Node it is three numbers, since npm reads `@1.2` as every `1.2.x`.

Both questions draw these lines: the one `bravebot mcp add` and `approve` ask, and the one a session
asks. What is read is the words the person declared and nothing else, so this says what the line asks
for and cannot say what the runner will find. A runner the list does not name, or a script that
calls one, is drawn as a plain program.

`verified-by: bravebot_cli::servers::a_runner_is_named_as_one_and_an_unpinned_package_as_unpinned`
`verified-by: bravebot_cli::servers::a_runner_and_its_unpinned_package_are_drawn_at_the_question`
`verified-by: bravebot_cli::servers::a_package_behind_a_flag_nobody_knows_is_drawn_as_not_known`
`verified-by: bravebot_cli::mcp::the_question_names_a_runner_and_the_package_it_leaves_unpinned`

<a id="SERVERS-7"></a>
### SERVERS-7: every call to a server's tool is put to the person, with three answers

A call is shown as `alias:tool`, then the arguments as the planner wrote them. The server's own
description of the tool is shown as content, marked, cut to two rows with a way to expand it, per
[SERVERS-8](#SERVERS-8).

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

Answer 2 records the alias, the tool and the project path. It reaches that one tool of that one
server in that one project, and says nothing about the same tool elsewhere, another tool of the same
server, or the arguments a later call carries.

A permission rule decides first, exactly as it does for a command line
([PERM-2](permissions.md#PERM-2)): a standing answer removes the default prompt and does not overrule
a rule a person wrote ([MODE-6](permission-modes.md#MODE-6) says the same of a mode).

**The routing field of a call is the alias and the tool, and never the arguments.**
[PERM-1](permissions.md#PERM-1) matches a specifier against a routing field and nothing else, for the
reason that routing is trusted before it reaches a gate while observed bytes are not. For a call the
alias is a name a person typed and the tool is a name a person can see in a list, so both are
decidable; the arguments are a payload the planner assembled, in part from content it read. A
specifier that matched them would be the driver branching on untrusted content, which is the thing
that clause exists to forbid. That is also what answer 2 can and cannot promise: it covers a
destination, not a payload.

[mcp.md](mcp.md)'s own reasoning is what makes this worth stating rather than assuming. It says an
MCP call is opaque, and that this "erases the split between the part of a call that decides where it
lands and the part that is merely carried". Defining the routing field as the alias and tool is how
that split is restored at the only boundary where it can be: outside the call, where the names are.
Inside it, the spec is right and nothing here recovers the distinction.

**A call carrying the person's own private data asks, whatever is standing.**
[PERM-9](permissions.md#PERM-9) already holds that a run which would put the user's private data into
a program asks whatever the rules say, because a rule about which commands may run is not consent to
hand one that data. A server is a program we did not write, and a remote one is a program on somebody
else's machine, so the same question applies with less excuse. A standing answer from answer 2 does
not cover it, and neither does a rule.

**Why the arguments do not bound the standing answer.** Binding it to them would make it useless,
since no two calls carry the same ones; binding it to the tool is the granularity a person reasons
at, which is "this server may tell me the weather without asking each time". A tool whose arguments
decide whether the call is safe is a tool that should not be standing-approved, and a `deny` rule is
how that is said.

**Why a call is asked about at all when the server was approved.** Approving a server is agreeing
that it may exist and be offered. A call is an effect: it leaves this machine, or it runs a program,
and it does so with arguments the planner chose from content it read. [MCP-2](mcp.md#MCP-2) already
refuses a call without the capability; this is the question the capability does not answer.

**How it is built.** The question is asked when the planner asks for a call, and in this order. A
`deny` rule naming the server or the tool refuses the call before anybody is asked, and the planner
is told not to retry it. Otherwise the call asks, unless an `allow` rule names it or answer 2 was
given about it in this project. An `ask` rule asks whatever answer 2 said. The rules are
[PERM-1](permissions.md#PERM-1)'s `Mcp` family. A call is refused as well, and sends nothing, where
the context its arguments were written in has met untrusted content.

The arguments are drawn as the planner wrote them, one per row as a JSON value. The description is
the one on the list the person vouched for, behind the margin and cut to two rows until `e`
expands it. `1` or `y` is answer 1. `2` is answer 2. `3`, `n` or Esc is answer 3. Enter answers
nothing, so a key pressed for the last question does not answer this one. Answer 3 sends nothing
and tells the planner the person declined.

Answer 2 is written to `mcp-tools` as the alias, the tool and the workspace root with its links
followed. Where the session has no state directory, or writes nothing as an incognito one does, it
is drawn as not offered and its key does nothing. A record that will not take it still makes the
call, and a line says the next call asks again. A record that is there and cannot be read, this one
or `mcp-approved`, is left as it is rather than written over with the one answer.

The plain interface asks the question as a line answered yes or no, and its yes is answer 1: a line
has room for one answer, and the one that stops asking outlives the session. A one-shot run has
nobody to ask, so a call there that would ask is refused. An `allow` rule decides nothing there, as
it decides nothing for any tool in a one-shot run, and answer 2 still does: it was given at this
question and names one tool in one project, where an `allow` rule is a line of a file that can name
every server. A delegate's call is put to the person as the turn's own is, after the same rules and
standing answers ([DELEGATE-10](delegation.md#DELEGATE-10)).

The private-data arm is reached once the planner has been shown private content let out of
quarantine, such as a server's answer a person vetted: its arguments are labelled private from then
on ([LABEL-8](labels.md#LABEL-8)), so every later call asks whatever a rule or a standing answer
says, and a one-shot run refuses it. A resumed session keeps this, and undoing the turn that let
the content out ends it. A workspace file the planner reads does not reach it.

`verified-by: bravebot_agent::mcp::a_vouched_list_offers_its_tool_and_a_call_answers_quarantined`
`verified-by: bravebot_agent::mcp::a_refused_call_reaches_no_server`
`verified-by: bravebot_agent::mcp::answer_two_stops_asking_for_the_one_tool_in_the_one_project`
`verified-by: bravebot_agent::mcp::answer_two_follows_the_session_to_another_project`
`verified-by: bravebot_agent::mcp::a_rule_decides_a_call_before_the_prompt`
`verified-by: bravebot_agent::mcp::a_session_that_writes_nothing_records_neither_answer`
`verified-by: bravebot_agent::mcp::a_call_after_a_servers_answer_was_vetted_asks_though_a_rule_allows_it`
`verified-by: bravebot_agent::mcp::reading_workspace_files_leaves_a_call_a_rule_allows_unasked`
`verified-by: bravebot_agent::mcp::a_record_that_cannot_be_read_is_not_written_over`
`verified-by: bravebot_agent::mcp::a_workers_call_to_a_servers_tool_is_put_to_the_person`
`verified-by: bravebot_core::policy::a_deny_rule_refuses_an_mcp_call_before_anybody_is_asked`
`verified-by: bravebot_core::policy::an_mcp_call_asks_unless_a_rule_or_a_standing_answer_says_otherwise`
`verified-by: bravebot_core::policy::private_arguments_ask_even_for_a_tool_a_rule_allows`
`verified-by: bravebot_core::policy::an_mcp_call_needs_the_endorsement_for_its_own_tool`
`verified-by: bravebot_core::policy::arguments_from_a_fallen_context_do_not_reach_a_server`
`verified-by: bravebot_core::permissions::an_mcp_rule_covers_the_server_or_the_tool_it_names`
`verified-by: bravebot_config::mcp::a_standing_answer_reaches_one_tool_of_one_server_in_one_project`
`verified-by: bravebot_config::mcp::a_standing_answer_reads_back_with_a_space_in_its_project`
`verified-by: bravebot_config::mcp::forgetting_a_project_drops_its_standing_answers_and_no_others`
`verified-by: bravebot_config::mcp::a_record_that_cannot_be_read_is_not_read_to_be_written_over`
`verified-by: bravebot_cli::mcp::forget_drops_a_projects_standing_answers_and_nobody_elses`
`verified-by: bravebot_cli::mcp::forget_takes_a_path_that_no_longer_resolves_as_typed`
`verified-by: bravebot_cli::mcp::forget_leaves_a_record_it_cannot_read_as_it_is`
`verified-by: bravebot_cli::mcp::forget_takes_one_path_at_most_and_writes_nothing_incognito`
`verified-by: bravebot_tui::confirm::a_call_prompt_draws_the_tool_its_arguments_and_three_answers`
`verified-by: bravebot_tui::confirm::a_call_prompt_cuts_a_long_description_until_it_is_expanded`
`verified-by: bravebot_tui::confirm::a_call_prompt_answers_by_its_rows_and_never_by_enter`
`verified-by: bravebot_tui::confirm::a_call_answer_says_what_the_turn_is_told`
`verified-by: bravebot_tui::remote_confirm::a_call_answer_travels_back_with_its_stand`

<a id="SERVERS-8"></a>
### SERVERS-8: a server's own words are never an identifier

A tool is named to the planner by the alias the person chose and the tool's name beneath it, as
`weather:get_current_conditions`. A server's self-reported name is display text and never the
namespace. A name that would collide with a built-in tool, or with another alias, does not shadow
it: the alias is what disambiguates, and an alias is a name a person typed.

A tool's description and its input schema are content from outside, so they are labelled untrusted
and quarantined like the results [MCP-1](mcp.md#MCP-1) already covers. Where a description reaches
the planner it is marked as content, inside a margin the renderer draws
([LAYER-3](layering.md#LAYER-3)), and it is never concatenated into the instructions that tell the
planner how to behave.

**Why.** A description is the one part of a server that is injected into the planner's context
before anybody has called anything, and it arrives from the same place a result does. Treated as
trusted text it is an instruction slot on the far side of every gate this repository has, reachable
by anyone who can publish a package. Letting a server pick its own namespace is the same hole a step
earlier: a server that calls its tool `write_file` is a server asking to be mistaken for a
primitive, and the tool surface the planner sees must be decided by names people chose.

**How it is built.** Listing a server's tools returns one list, labelled untrusted, that the client
drew from the server's reply and keeps nothing else of. Per tool it holds the word, the description,
and each argument as its name, its type, the values it may take where it names at most sixteen, and
whether it is required. An argument's own description is dropped: nobody reads it at the list, and
it would still reach the planner. In a description, each control character but a line break, and
each character that reorders or hides text, is made a space, and it is cut at 1024 characters.

Some tools are refused rather than offered, and counted without being named: a word that is not a
function name, one whose name on the wire would be longer than 64 characters, a word listed twice
(every copy), and a tool whose arguments are not names. A list longer than 128 tools keeps the first
128. A list sent in pages is read a page at a time, each asked for with the cursor the page before
it named, and no more than 8 pages are read, so a server with more lists what those held. The list
is sorted by word, so within the pages read the order a server sent it in changes nothing. A server that
answers `tools/list` with JSON-RPC's method-not-found lists no tool, since one serving only
resources or prompts need not have it, and any other failure of the method fails the handshake.

That list is put to the person at the start of the first turn they ask for, whole. It shows each
tool's name as `alias:tool`, its arguments, and every row of its description behind the margin with
none of it cut, since a yes puts exactly this text in front of the planner. A check reads it first,
as it reads anything else about to be promoted ([CHECK-10](vetting.md#CHECK-10)), and its verdict is
drawn above the list.

A yes is the person vouching for the list, and it is the one road by which the list is promoted
([LABEL-8](labels.md#LABEL-8)). A no offers none of its tools for the rest of the session and is not
asked again. A turn stopped at the question has not answered it, so the next turn asks again. The list's digest is recorded as a `tools` line in `mcp-approved`, beside the digest of
the declaration it was listed under. A later session whose server sends the same list offers it with
nobody asked. One whose list changed says so, and asks again. A project path that
[SERVERS-4](#SERVERS-4)'s answer 2 recorded answers for a server and not for its list.

Each tool is offered as a function named `mcp__{alias}__{tool}`, which is what a backend's function
names allow. Its description is a sentence this process writes, naming the alias and saying every
call is put to the person, followed by the server's words with the margin before each line. No
built-in's name starts with `mcp__`, so none can be shadowed. Two servers whose alias and word
compose one name are both left without a tool of that name, since which of them a call reached would
be the order they were listed in. A delegate, and a turn addressed to a definition, is offered the
tools of only the servers it holds a grant for ([SERVERS-9](#SERVERS-9)). A delegate is put no list
and is offered what the turn that spawned it settled. An addressed turn is put the lists of only the
servers it holds a grant for, and another server's list waits for a turn that holds that grant.

`verified-by: bravebot_mcp::protocol::a_tool_list_is_content`
`verified-by: bravebot_mcp::protocol::the_name_on_the_wire_is_the_alias_and_the_word`
`verified-by: bravebot_mcp::protocol::a_word_that_is_not_a_function_name_is_refused_and_counted`
`verified-by: bravebot_mcp::protocol::a_word_too_long_for_the_wire_is_refused`
`verified-by: bravebot_mcp::protocol::a_word_listed_twice_is_refused_both_times`
`verified-by: bravebot_mcp::protocol::a_tool_whose_arguments_are_not_names_is_refused`
`verified-by: bravebot_mcp::protocol::a_description_is_blanked_and_cut`
`verified-by: bravebot_mcp::protocol::a_character_nobody_sees_is_blanked`
`verified-by: bravebot_mcp::protocol::an_argument_is_its_name_type_and_whether_it_is_required`
`verified-by: bravebot_mcp::protocol::the_list_is_the_same_whatever_order_the_server_sent`
`verified-by: bravebot_mcp::protocol::a_long_list_is_capped_and_the_rest_counted`
`verified-by: bravebot_mcp::protocol::a_server_without_the_method_lists_no_tool`
`verified-by: bravebot_mcp::protocol::a_list_in_pages_is_read_to_its_last_page_and_no_further_than_the_bound`
`verified-by: bravebot_mcp::stdio::a_list_in_pages_is_offered_whole`
`verified-by: bravebot_mcp::protocol::printing_a_listing_does_not_print_what_the_server_said`
`verified-by: bravebot_mcp::stdio::a_confined_server_completes_the_handshake_and_lists_tools`
`verified-by: bravebot_mcp::http::a_handshake_and_tool_list_round_trip`
`verified-by: bravebot_config::mcp::a_tool_word_is_what_a_function_name_may_be`
`verified-by: bravebot_config::mcp::a_vouched_list_reads_back_beside_the_approval_it_was_listed_by`
`verified-by: bravebot_config::mcp::a_vouched_list_lasts_as_long_as_its_declaration_resolves`
`verified-by: bravebot_config::mcp::a_line_that_is_not_a_vouch_is_passed_over`
`verified-by: bravebot_core::policy::a_tool_list_reaches_the_planner_only_through_an_endorsement`
`verified-by: bravebot_core::policy::a_recorded_tool_list_is_promoted_only_where_it_is_the_one_vouched_for`
`verified-by: bravebot_core::policy::a_check_before_a_tool_list_reads_the_list_it_is_about`
`verified-by: bravebot_agent::mcp::a_vouched_list_offers_its_tool_and_a_call_answers_quarantined`
`verified-by: bravebot_agent::mcp::a_servers_tool_named_like_a_built_in_one_shadows_nothing`
`verified-by: bravebot_agent::mcp::two_servers_composing_one_name_offer_neither_under_it`
`verified-by: bravebot_agent::mcp::a_declined_list_offers_nothing_and_is_not_asked_again`
`verified-by: bravebot_agent::mcp::a_turn_stopped_at_the_list_leaves_it_to_be_asked_again`
`verified-by: bravebot_agent::mcp::a_list_vouched_for_before_asks_nothing_and_a_changed_one_asks_again`
`verified-by: bravebot_agent::mcp::a_workers_call_to_a_servers_tool_is_put_to_the_person`
`verified-by: bravebot_agent::mcp::a_reader_or_a_checker_delegate_holds_no_server_and_reaches_none`
`verified-by: bravebot_tui::confirm::a_tool_list_draws_every_description_row_behind_the_margin`
`verified-by: bravebot_tui::confirm::a_tool_list_answers_by_its_rows`
`verified-by: bravebot_tui::remote_confirm::a_yes_to_a_list_and_a_yes_to_a_call_do_not_stand_in_for_each_other`

<a id="SERVERS-9"></a>
### SERVERS-9: the capability is granted per server, not per protocol

[MCP-2](mcp.md#MCP-2) refuses a call without the capability. The capability names an alias, so
granting calls to one server grants nothing about another, and a declaration that has never been
granted is reachable by nobody.

Removing a grant takes effect on the next call rather than at the next session, since a grant is the
thing being asked about and not a property the session recorded.

**Why.** A single capability covering everything behind this boundary would make adding a second
server a widening of what the first may be asked to do, which is the opposite of what adding a
server should mean.

The capability names the alias, both transports gate on the one naming the server in front of
them, and a grant can be withdrawn while the run is going. A session holds one grant for each
server it started and no other. A delegate it hands work to holds those of the grants its
definition selects where the delegate is a worker, which is every one where the definition names
neither its tools nor its servers, and none where it is a reader or a checker
([DELEGATE-4](delegation.md#DELEGATE-4), [DELEGATE-24](delegation.md#DELEGATE-24)): each is a
grant for a server a person already said the session may use, and a server's tool may do what only
a worker may. No delegate holds a grant its
parent does not. A turn addressed to a definition holds the session's grants on the same terms
([ADDRESS-7](addressing-a-definition.md#ADDRESS-7)). A call to a server's tool is refused where the
run holds no grant naming that server, and a remote server's handshake runs under a policy holding
the grant naming that server and no other.

`verified-by: bravebot_core::capability::a_grant_for_one_server_is_not_a_grant_for_another`
`verified-by: bravebot_core::capability::withdrawing_one_grant_leaves_the_others`
`verified-by: bravebot_mcp::stdio::a_grant_for_one_server_does_not_reach_another`
`verified-by: bravebot_mcp::http::a_grant_for_one_server_does_not_reach_another`
`verified-by: bravebot_mcp::stdio::a_grant_withdrawn_stops_the_next_call`
`verified-by: bravebot_agent::turn::a_turn_holds_a_grant_per_server_it_was_handed_and_only_its_worker_holds_them_too`
`verified-by: bravebot_core::delegate::only_a_worker_holds_the_servers_of_the_run_it_is_carved_from`
`verified-by: bravebot_core::delegate::a_worker_holds_no_server_its_parent_does_not`
`verified-by: bravebot_core::policy::a_worker_delegate_holds_the_servers_its_parent_holds_and_no_other`
`verified-by: bravebot_agent::mcp::a_reader_or_a_checker_delegate_holds_no_server_and_reaches_none`
`verified-by: bravebot_agent::mcp::a_worker_is_offered_only_the_servers_its_parent_holds`
`verified-by: bravebot_core::delegate::a_worker_naming_servers_holds_only_those_of_them_its_parent_holds`
`verified-by: bravebot_core::policy::a_definition_naming_servers_is_delegated_with_only_those_its_parent_holds`
`verified-by: bravebot_agent::mcp::a_worker_whose_definition_names_one_server_is_offered_only_its_tool`

<a id="SERVERS-10"></a>
### SERVERS-10: a variable a server needs is named or stored in the declaration, and reaches that server alone

[MCP-9](mcp.md#MCP-9) empties the environment before a stdio server starts, on every platform, and
its known cost is that a server needing a variable to work at all does not work. This is that cost
paid: the declaration lists variable **names**, read from this process's environment at launch,
and may store a value for others, and the resulting environment of the server is those values and
nothing else, beside the `HOME` described below.

A value is stored only where the person gave it: typed at `add` as `-e NAME=value`, or written into
`~/.bravebot/mcp.json` as an `env` object of names and values. A settings layer supplies none
([SERVERS-1](#SERVERS-1)). A value written in place of a name in `variables` is a parse error that
does not repeat it, and so is a name both stored and listed there, which is two answers to where its
value comes from, and a stored value no environment could hold. `PATH` is a name like any other, so
a server whose program must be resolved says so.

The names appear at [SERVERS-4](#SERVERS-4)'s prompt, a stored one marked as stored. The values,
stored or read, appear in no prompt, no log, no session record, and no `get`, `list` or `doctor`
output.

**Why.** A server's own install line is written for Claude Code's `-e`, and refusing it leaves a
person nothing to type but an export into the shell, where every program they run reads the key
rather than the one server that needs it. The value is kept where the declaration already is, in the
state directory that only its owner can read ([STATE-1](state-directory.md#STATE-1)), and never in a
settings layer: [BACKEND-1](backends.md#BACKEND-1)'s reasoning holds there, since those are the
files on the machine that are easiest to write to. Passing only the named and stored ones keeps
[MCP-9](mcp.md#MCP-9)'s property where it matters: this process's own credentials are in variables,
and a server is code we did not write.

`-e` and `--env` take the words after them up to the next flag, as Claude Code's do, and `--env=` or
`-e` joined to a word takes that word alone. Each `NAME=value` stores the value, split at its first
`=`. A name given alone as the one word its `-e` took is a name read from the environment. Any other
word, a name in a run of several included, is refused by its place in what was typed and never
repeated, since `-e TOKEN sk-live` would otherwise store nothing and print the key. An `-e` before
the alias is refused too, where Claude Code's own run would take the alias as a value, and a name
given twice is refused by the name. A word holding an `=` where the alias, a `-s` or a `--dir` takes
its word is refused without being repeated, as a forgotten alias or `-e` leaves one there. A program
`add` is given by a bare name, such as `npx`, has `PATH` added to the names once, whether or not it
was typed, unless a `PATH` was stored, since the `PATH` a declaration gives is the only one such a
program is looked for in, as the next paragraph says. It is shown at the question as a typed name
is, and is part of what the digest covers. A program given as a path is declared with only the names
typed.

At launch each stored value is handed over as stored, and each named variable this process holds is
handed over, one it does not hold being left out rather than set empty. A program named rather than
given as a path is looked for in the `PATH` the declaration gives and in no other, since that is the
one the server runs with: without `PATH` among the names or the stored values it is not found, and
the line says to declare `--env PATH` or give the program as an absolute path. A relative path is
refused, since it names a different program in each directory.

**A file a stored value or an argument names is one the server may read.** A server's install line
names its key file this way, as brave-search's `-e BRAVE_API_KEY_FILE=/path/to/key`, and a key the
person keeps in their home directory is otherwise out of the server's reach. `add` takes each stored
value and each argument that is an absolute path to a file that exists now, with its links followed,
and where the confinement below would not reach it, records it in the declaration's `reads`, which
the question shows and the digest covers. A directory is never recorded, since a word naming one is
too broad a read to infer, and neither is a file in the state directory, so a line copied from
somewhere else cannot hand a server the person's declarations, approvals or credentials. A relative
word, a missing path and a file the confinement reaches already record nothing.

The process is confined under [MCP-3](mcp.md#MCP-3), and what it may reach is built for it:

- The sandbox's base rows for this platform, which allow egress and children, with no home
  directory given, so the git configuration in it is not among them either. Egress is to IP
  addresses, and a unix socket is reached only under a path the server may write
  ([SANDBOX-3](sandboxing.md#SANDBOX-3)), the temporary directory among them. A server that runs
  `docker` against the daemon's usual socket does not reach it, and one reaching a daemon on a
  loopback address does, since that is IP.
- Read access to each directory the named `PATH` searches and the one the program resolved into,
  with their links followed, since a runner is a script whose interpreter is found through `PATH`.
  A `bin` directory brings its parent, where an installation keeps what its programs load.
- The home directory itself is not among them: a `PATH` entry naming it, or a directory above it,
  is left out, and so is a `bin` directory's parent inside it, since `~/.cargo` keeps a registry
  token beside `~/.cargo/bin`. A `PATH` entry inside it, such as `~/.local/bin`, is read, as the
  place the person put programs.
- The installation the program came from, which is the directory above the `bin` directory the
  program resolved into, with its links followed, where that sits deeper in the home directory than
  a directory directly inside it. `nvm` installs `npx` as a link into
  `~/.nvm/versions/node/<version>/lib/node_modules/npm/bin`, and the script there loads the rest of
  npm from beside it. A `bin` directory directly inside a directory at the top of the home, such as
  `~/.cargo/bin`, brings nothing, for the reason above.
- The server's own directory, below, which it may read and write.
- Each file the declaration's `reads` names, read-only and as that one file: never the directory it
  is in, and nothing where the path is no longer a file, or is a link or is reached through one,
  which is not the file the person was shown, or is in the state directory, which only a hand edit
  of the file can have declared.
- The declared directory, which the server may read and write and starts in. Without one it starts
  in the system temporary directory, and reads nothing of the workspace.
- A look at any path, and no read or listing beyond the rows above, which is
  [SANDBOX-13](sandboxing.md#SANDBOX-13)'s: node resolves its own script through each directory
  above it, and a server that cannot is not started by any runner.

The home these rows keep out is the person's profile directory, and not the state directory inside
it that bravebot keeps its own files in. It is compared with its links followed, as the rows are,
so a home reached through a link is kept out as well.

**A server is given no directory inside a git repository.** git runs the commands a repository's
configuration and hooks name, unconfined, the next time anybody runs git there, and a relative
`core.hooksPath` makes files of the work tree into hooks, so a server that may write a repository
may run code outside its confinement. A declared directory git would open a repository from is
refused: one with a `.git` in it or in a directory above it, or one laid out as a repository
itself, a `HEAD` file beside `objects` and `refs` directories. `add` refuses it as an argument and
writes nothing, and a session refuses a declaration that names one before anybody is asked, with a
line naming the directory and the repository it is in. On macOS a row the server may write does
not reach a `.git` beneath it either, whatever its case ([SANDBOX-14](sandboxing.md#SANDBOX-14)),
so a server cannot make one in its own home or the temporary directory and write into it.

**A server has a home directory of its own**, and `HOME` names it unless the declaration names
`HOME`, in which case the person's value is handed over as any other named variable is. A runner
keeps its cache and a server its settings under `HOME`, and with the person's own home out of reach
and no `HOME` at all, `npx` cannot start. The directory is
`~/.bravebot/mcp-home/<digest>`, keyed by the declaration's digest rather than its alias, so what a
runner fetched is there on the next launch and a declaration edited to run another program starts
with nothing the one before it wrote. It is created by this process before the server starts, at
the modes [state-directory.md](state-directory.md#STATE-1) gives them, and a server whose directory
cannot be created is not started, and the line says where it was to be made and why.

It is not trusted, and `~/.bravebot` is exactly why somebody will think it is, as with
[LSP-10](tools/lsp.md#LSP-10)'s cache. [TRUST-11](trust-map.md#TRUST-11) makes that directory
trusted because the person wrote what is in it, and this directory is written by a server, which is
code we did not write. The driver never opens it: what it reads under `~/.bravebot` is what
[instructions.md](instructions.md) and [skills.md](skills.md) name, and this is none of it. A read
the planner asks for reaches it only where [TRUST-10](trust-map.md#TRUST-10) lets one, which is a
workspace holding it, and is read through the map like any other file there.

**A session that keeps nothing** hands the server a directory of its own in the system temporary
directory instead, on the terms [incognito.md](incognito.md#INCOG-8) states for it: created rather
than adopted, readable by nobody else, and removed once the server has stopped. A runner there
fetches its package again each session, which is the trade incognito already makes.

A platform with no confinement for this, which is Windows today, starts no stdio server and says
so, and asks nobody about one it could not start. Where the sandbox cannot be built, the server is
not started either, and the line says why.

`verified-by: bravebot_config::mcp::a_value_is_stored_under_its_name_and_one_written_as_a_name_is_refused_unrepeated`
`verified-by: bravebot_config::mcp::an_env_block_is_stored_values_and_an_object_of_variables_is_not_a_list_of_names`
`verified-by: bravebot_config::mcp::a_stored_value_no_environment_could_hold_or_a_second_answer_is_refused_by_its_name`
`verified-by: bravebot_config::mcp::a_name_that_is_not_one_is_refused`
`verified-by: bravebot_config::mcp::a_remote_server_takes_no_variables_values_reads_or_directory`
`verified-by: bravebot_config::mcp::a_stored_value_is_debugged_by_its_name_alone`
`verified-by: bravebot_cli::running::a_value_given_at_add_is_stored_where_only_the_person_reads_it_and_printed_by_nothing`
`verified-by: bravebot_cli::running::a_value_add_cannot_take_is_refused_and_never_repeated`
`verified-by: bravebot_cli::running::a_servers_own_install_line_naming_a_key_file_declares_a_read_of_that_file`
`verified-by: bravebot_cli::mcp::every_way_claude_code_takes_a_value_stores_it`
`verified-by: bravebot_cli::mcp::a_word_an_env_flag_cannot_read_is_named_by_its_place_and_never_repeated`
`verified-by: bravebot_cli::mcp::a_value_before_the_alias_is_refused_and_nothing_is_written`
`verified-by: bravebot_cli::mcp::a_value_where_an_alias_a_scope_or_a_directory_goes_is_refused_and_never_repeated`
`verified-by: bravebot_cli::mcp::a_stored_value_is_shown_by_its_name_and_never_as_itself`
`verified-by: bravebot_cli::mcp::a_file_a_value_or_an_argument_names_is_declared_as_a_read_and_nothing_broader_is`
`verified-by: bravebot_cli::servers::a_stored_value_is_handed_to_the_server_and_the_environment_is_not_read_for_it`
`verified-by: bravebot_cli::servers::a_declared_read_grants_the_one_file_and_nothing_beside_it`
`verified-by: bravebot_cli::servers::a_started_server_reads_the_files_it_was_declared_to_and_nothing_beside_them`
`verified-by: bravebot_cli::mcp::a_program_named_by_a_bare_name_is_declared_with_path`
`verified-by: bravebot_cli::mcp::the_question_shows_the_path_a_bare_name_was_given`
`verified-by: bravebot_cli::servers::a_program_is_found_in_the_path_it_names_and_nowhere_else`
`verified-by: bravebot_cli::servers::a_path_the_declaration_does_not_name_resolves_nothing`
`verified-by: bravebot_cli::servers::a_local_server_is_not_asked_about_where_nothing_can_confine_it`
`verified-by: bravebot_cli::servers::a_servers_confinement_reaches_its_installation_and_nothing_of_the_home_directory`
`verified-by: bravebot_cli::servers::the_home_kept_out_of_a_launched_server_is_the_persons_and_not_the_state_directory`
`verified-by: bravebot_cli::servers::a_home_reached_through_a_link_is_kept_out_of_a_servers_confinement`
`verified-by: bravebot_cli::servers::a_programs_own_installation_deep_in_the_home_directory_is_read`
`verified-by: bravebot_cli::servers::a_server_is_handed_its_own_home_unless_the_declaration_names_one`
`verified-by: bravebot_cli::servers::a_server_keeps_a_home_of_its_own_under_the_state_directory`
`verified-by: bravebot_cli::servers::a_started_server_writes_its_own_files_in_the_home_kept_for_it`
`verified-by: bravebot_cli::servers::a_server_in_a_session_that_keeps_nothing_is_given_a_home_that_goes_with_it`
`verified-by: bravebot_cli::servers::a_started_server_in_a_session_that_keeps_nothing_has_a_home_that_goes_with_it`
`verified-by: bravebot_agent::mcp::a_servers_throwaway_home_lasts_as_long_as_the_session_holding_it`
`verified-by: bravebot_mcp::stdio::a_server_receives_the_variables_it_was_handed_and_no_others`
`verified-by: bravebot_cli::servers::a_declared_directory_holding_a_repository_is_refused`
`verified-by: bravebot_cli::mcp::add_declares_no_directory_inside_a_repository`
`verified-by: bravebot_sandbox::macos::a_write_row_does_not_reach_a_git_directory_beneath_it`

<a id="SERVERS-11"></a>
### SERVERS-11: a remote server is an egress destination, and its host is approved as one

An HTTP server has no process to confine, so [MCP-3](mcp.md#MCP-3) has nothing to say about it and
the boundary is the network instead. A call to one passes the policy gate in `bravebot-net` like
everything else carrying labelled content, the host is part of the digest, and a hop that leaves
where the server was declared is put to the person rather than followed. Where they approve it, the
declaration is rewritten to the destination they were shown, so a server that has moved is one
somebody wrote down again and the call after it asks nothing. Where they do not, nothing is sent.

Where a server is is a socket and not a name, so a hop that changed the port has left it as surely
as one that changed the host. A declaration is a url somebody wrote in full, and the machine it
names runs other services on other ports: a loopback declaration compared on the host alone would
deliver a tool call to whatever else answers at that address.

Nothing in the settings answers that question in advance. A `WebFetch` rule names websites the
planner may reach, and what goes out here is one server's call with its arguments and its session
id, so a rule about a host is not consent to hand those to a different service.

The destination is shown where a server's bytes may be shown and nowhere else. It reaches the
prompt, which is drawn for a person and parsed by nothing afterwards, and it does not reach the
refusal the planner is given: that names the declared destination, which is what somebody wrote
down ([LABEL-3](labels.md#LABEL-3)).

A call to one is a client in this process like any other, so the handshake is validated against the
roots the machine states rather than this build's alone ([NET-7](network-egress.md#NET-7)) and it
takes the proxy the machine names ([NET-8](network-egress.md#NET-8)). A declaration carries a url and
never a root, a proxy or a certificate: those are the machine's answer and not a server's to give.

**Why.** The two transports fail in opposite directions and it would be easy to write this spec as
though confinement covered both. A stdio server is dangerous because it runs here; a remote one is
dangerous because the content leaves. Naming the host in the digest is what stops an approved alias
from being repointed at a different service by an edit that keeps the argv shape intact.

**Why a question and not a follow.** A move and a redirection are written the same way. The client
resends the request it was holding, so following a hop sends the tool call, its arguments and the
session id onward, and the header that decided where is a server's own bytes: a server that has
genuinely moved and a server that wants the call delivered somewhere else are indistinguishable in
it. That is a difference only the person who declared the server can make, which is what makes this
a prompt rather than a rule.

**Why a question and not a refusal.** Servers move, and a deployment answering a move with a
relocation is doing the ordinary thing. A boundary that has no answer but no is one a deployment
works around, and a boundary everybody works around is off.

**How it is built.** A session reaches an approved remote server as it opens, and its handshake
passes the egress gate under a policy holding the fetch capability and the grant naming that server,
and no other. Every call after it passes the same gate. The gate allows or refuses and cannot ask,
so a hop that leaves the declared destination, whether a call's or the handshake's, is refused there
and nothing is sent. The policy keeps where it pointed, as the server's bytes, for the prompt.

The prompt draws the declared url, where the reply pointed, and the host that reaches, with the port
where the url names one. In the full-screen interface, that host and what a yes does are drawn under
the destination in rows the destination cannot take, and a destination longer than the box scrolls
above them ([PROMPT-4](prompting.md#PROMPT-4)). `y` is not taken at a draw that cuts off the
declared url, that host, what a yes does or the keys, or that draws none of the destination's url. A
call's hop is asked during the turn, in the full-screen interface and in lines alike; a handshake's
is asked at the terminal as the session opens, and `y` is the only yes. Until the yes, the
destination decides nothing. After it, the url is read as a declaration like any other, so one that
cannot be declared is refused, and so is one whose host the managed layer refuses
([SERVERS-12](#SERVERS-12)). The handshake is then made there, and only once it completes is
`~/.bravebot/mcp.json` rewritten and the new digest recorded in `mcp-approved` in place of the old
one, so a destination that does not answer is never written down. A declaration edited while the
person was asked is left as it is, and the server is not moved. The call is then made again where
the server now is, once: a second hop off the new destination is refused and not asked about. A
line then says the server moved, since the move outlasts the prompt.

A project entry in `mcp-projects` answers for what a checkout requests and never for where a server
went, so it does not answer this question. Where the session writes nothing, the prompt says so and a
yes moves the server until the session ends. A vouched list is not carried to the new declaration,
since a list is vouched for beside the digest it was read under. The session that moved the server
goes on offering the list it already offered, and the next session to start the server puts its
list to the person again.

A no rewrites nothing and sends nothing. A call's hop then says the server stays where it is
declared, and a handshake's leaves the server unstarted with a line saying why. A one-shot run, a
session with nobody at the terminal and the mode that skips prompts refuse it unasked
([SERVERS-13](#SERVERS-13)).

`verified-by: bravebot_mcp::http::mcp_traffic_passes_through_the_network_gate`
`verified-by: bravebot_mcp::http::a_redirect_to_another_host_is_refused`
`verified-by: bravebot_mcp::http::a_redirect_within_the_declared_host_is_followed`
`verified-by: bravebot_mcp::http::a_failed_server_request_stops_confining_the_turns_other_egress`
`verified-by: bravebot_core::policy::a_rule_does_not_let_a_servers_request_be_redirected_off_its_host`
`verified-by: bravebot_core::policy::a_servers_request_cannot_be_redirected_to_another_port_on_the_same_host`
`verified-by: bravebot_core::policy::a_refused_hop_off_a_server_is_kept_untrusted_for_the_prompt`
`verified-by: bravebot_core::policy::a_server_move_is_promoted_only_through_an_endorsement`
`verified-by: bravebot_agent::mcp::a_hop_the_person_says_is_a_move_is_declared_and_the_call_reaches_it`
`verified-by: bravebot_agent::mcp::a_hop_the_person_refuses_sends_nothing_and_the_refusal_names_the_declaration`
`verified-by: bravebot_agent::mcp::a_move_the_managed_layer_denies_is_refused_whatever_is_answered`
`verified-by: bravebot_agent::mcp::a_move_in_a_session_that_writes_nothing_lasts_for_the_session`
`verified-by: bravebot_agent::mcp::a_move_is_recorded_over_the_declaration_asked_about_and_nothing_else`
`verified-by: bravebot_cli::servers::a_handshake_redirected_off_its_declaration_is_moved_on_a_yes`
`verified-by: bravebot_cli::servers::a_handshake_redirected_off_its_declaration_starts_nothing_on_a_no`
`verified-by: bravebot_cli::servers::a_move_in_a_session_that_writes_nothing_is_for_the_session`
`verified-by: bravebot_cli::servers::a_project_that_answers_for_its_servers_does_not_answer_a_move`
`verified-by: bravebot_cli::plain::a_move_is_asked_in_lines_and_only_a_yes_moves_the_server`
`verified-by: bravebot_tui::confirm::a_move_prompt_shows_the_declaration_the_destination_and_what_it_reaches`
`verified-by: bravebot_tui::confirm::a_destination_longer_than_the_move_box_leaves_the_host_and_what_a_yes_does_on_screen`
`verified-by: bravebot_tui::confirm::the_end_of_a_long_destination_can_be_scrolled_to_with_the_host_still_shown`
`verified-by: bravebot_tui::confirm::a_move_question_takes_a_yes_only_from_a_draw_showing_the_host_and_the_keys`

<a id="SERVERS-12"></a>
### SERVERS-12: the managed layer may keep a server from starting and never add one

An administrator's layer may say which servers may start and which may not, by the host a remote
one reaches or the command a local one runs. It may not declare one, approve one, grant a capability
to one, or pre-answer any of the three prompts.

A server the layer refuses is not started in any mode. Nothing is asked about it and nothing is
recorded for it. The session says, once as it opens, that it was not started, which file refused it
and why: that the file's allow list does not name it, or the deny entry that does. `bravebot mcp
list` and `get` say the same beside whatever the person's own declaration and approval say, `add`
and `enable` say it after writing a request for one, and `doctor` names `mcp.allow` and `mcp.deny`
among the names the layer pins.

**Why.** The machine-level layer exists to make an approved destination the only destination, and
the names it may pin are the ones that decide where a request goes. A layer that could add a server
would be a layer that installs a program on every machine it reaches, which is a much larger power
than the inversion that layer was built for, and `managed.rs` already argues the general form: a
layer that can pin anything is a layer somebody uses to pin a preference.

**Why not an alias.** The person who declares a server chooses its alias, so a list of aliases
bounds nothing: the same argv declared under another name starts as before. What an administrator
is deciding about is where a server connects and what it runs, and those are what an entry names.

**How it is built.** The keys are `"mcp": { "allow": [...], "deny": [...] }` in `managed.json`, the
file [BACKEND-38](backends.md#BACKEND-38) reads. An entry is an object with one key:

- `{"host": "mcp.corp.example"}` matches a remote server whose url names that host, whatever its
  port and path. Both sides are compared lowercased with one trailing dot dropped. `*.` may open
  the entry as a whole first label: `*.corp.example` matches `mcp.corp.example` and
  `a.b.corp.example`, and matches neither `corp.example` nor `evilcorp.example`, since a match inside
  a label is one a person could satisfy with a name they registered.
- `{"command": ["/usr/local/bin/approved-server", "--stdio"]}` matches a local server whose argv is
  those words exactly, once its program is the path the session resolved it to through the `PATH`
  its declaration names ([SERVERS-10](#SERVERS-10)). The first word must be an absolute path, since
  a resolved program is never anything else.

Without an `allow` list, every server not denied starts as it did. With one, a server no entry names
is refused, and `"allow": []` refuses every server. A deny entry wins over an allow entry naming the
same server. Only a list counts: a string, an object, `null` or anything else under either key
decides nothing, and an empty `deny` denies nothing, which `doctor` does not report either. An entry
in neither form, an alias among them, is skipped. In a deny list it denies nothing; in an allow list
it allows nothing, so an allow list of nothing else still refuses every server. The layer has no
field that could hold a declaration, an approval or a request, so an `mcp.request`, an
`mcp.approve`, a server block or an `mcpServers` object in that file is read as nothing at all.

A url's host is read only where it is spelled plainly: labels of ASCII letters, digits, `-` and
`_`, or an address in brackets, then a port of digits or none. The HTTP client reads the url with its
own parser, and a host read here that differed from the one it connects to would let a url match an
entry it does not reach; a backslash, a percent escape, an `@`, a second colon or a letter outside
those is where two parsers part. A url spelled any other way matches no allow entry, and a deny list
naming any host refuses it, since whether it reaches a denied host cannot be told.

The check is made once the declaration resolves and before anything is asked, recorded or started,
so [SERVERS-13](#SERVERS-13)'s mode reaches a refused server no more than an answer would, and
nothing is written to `mcp-approved` or `mcp-projects`. An approval the person already holds is left
where it is: the refusal is the administrator's, and taking it away brings the server back as it
was answered for.

The same keys in a settings layer are not a declaration, so `doctor` does not name them, and they
refuse nothing: the person's own road to a server gone is `bravebot mcp remove`, and a checkout's is
not to request it.

**Known costs.**

- A command entry binds a path, not a program. A link, a `..`, a copy under another name or, where
  the filesystem ignores case, another spelling of the same file is another argv, so each one evades
  a deny entry. An allow list holds against all of them, since it names what may start, as long as
  the person cannot write to the path it names.
- A host entry names a spelling. Another name for the same machine, or its address, is another host,
  so a deny entry is evaded by either, and the allow list is again the form that holds.
- A command match reads the argv and nothing else. The variables and the directory a declaration
  names are not part of it, so an allowed program can be started with an environment that changes
  what it does, a loader variable among them.

`verified-by: bravebot_config::managed::an_allow_list_starts_only_what_it_names`
`verified-by: bravebot_config::managed::an_empty_allow_list_starts_nothing`
`verified-by: bravebot_config::managed::a_deny_entry_wins_over_an_allow_entry`
`verified-by: bravebot_config::managed::without_an_allow_list_only_what_is_denied_is_refused`
`verified-by: bravebot_config::managed::a_host_two_parsers_could_read_apart_matches_no_entry`
`verified-by: bravebot_config::managed::a_list_that_is_not_a_list_decides_nothing`
`verified-by: bravebot_config::managed::an_entry_in_neither_form_is_skipped`
`verified-by: bravebot_config::managed::a_server_declared_or_requested_here_is_read_as_nothing`
`verified-by: bravebot_config::settings::a_request_or_a_server_list_in_the_mcp_block_is_not_a_declaration`
`verified-by: bravebot_cli::servers::a_server_the_managed_layer_refuses_is_started_in_no_mode_and_nothing_is_asked_or_recorded`
`verified-by: bravebot_cli::servers::a_server_the_managed_layer_does_not_refuse_is_settled_as_before`
`verified-by: bravebot_cli::servers::a_remote_server_is_refused_by_its_host`
`verified-by: bravebot_cli::mcp::list_and_get_say_why_the_managed_layer_refuses_a_server_and_no_other`
`verified-by: bravebot_cli::mcp::enabling_a_server_the_managed_layer_refuses_says_it_is_not_started`

<a id="SERVERS-13"></a>
### SERVERS-13: bypassing answers this spec's three prompts and reaches nothing else here

[MODE-4](permission-modes.md#MODE-4) already decides the first half: `--dangerously-skip-permissions`
answers every permission question. [SERVERS-4](#SERVERS-4)'s question about a server,
[SERVERS-8](#SERVERS-8)'s about its list and [SERVERS-7](#SERVERS-7)'s about a call are permission
questions, so each is answered yes without being drawn. This clause does not grant that
and could not withhold it. It says what the mode does **not** reach, which is everything else in this
spec, and it is [MODE-7](permission-modes.md#MODE-7)'s rule applied here: no mode answers a question
that is not a permission, and most of what protects a person from a server is not a question at all.

The mode is reachable only where the command line asked for it
([MODE-5](permission-modes.md#MODE-5)), so nothing in a declaration, a request key or a settings
layer turns it on.

Each of the following holds in that mode exactly as it holds outside it:

| Still in force | Why it is not a prompt |
|---|---|
| [MCP-3](mcp.md#MCP-3), a stdio server is not launched without confinement | A server that cannot be confined is not started, in this mode too. Confinement is not a question anybody was being asked. |
| [MCP-9](mcp.md#MCP-9) and [SERVERS-10](#SERVERS-10), the environment is the named and stored variables and nothing else | The prompt showed the names. Skipping the showing does not widen the set. |
| [MCP-1](mcp.md#MCP-1), a result is untrusted | A label is not an approval. Nothing a person could have said at a call would have made what it returned trusted, so there is nothing here for a skipped question to have granted. |
| [SERVERS-1](#SERVERS-1), only the person's own directory declares a server | An undeclared server does not become declared by nobody being asked about it. A checkout's request still resolves against declarations or resolves to nothing. |
| [SERVERS-9](#SERVERS-9), the capability | A capability is configuration, not a prompt. A server with no grant is called by nobody in this mode either. |
| [SERVERS-12](#SERVERS-12), the managed allow and deny lists | An administrator's refusal is not a question being put to the person running the program. |
| [SERVERS-11](#SERVERS-11), the egress gate and the host in the digest | The gate decides on labels. This mode answers three named questions and not every question. Whether a server moved is a fourth, and a yes to it would rewrite the declaration, the one file a session cannot write, to a url a server wrote and nobody read. So it is refused unasked and nothing is sent where the hop pointed. |

**Nothing is recorded.** A skipped question leaves no approval, no vouched list, no project path and
no tool entry behind, so a later run outside the mode asks every question as though this one had not happened. That
is not a new rule: [MODE-4](permission-modes.md#MODE-4) already says a run approved this way vouches
for no program, and that a record claiming somebody approved programs they were never shown would be
a standing permission nobody granted. A digest in `mcp-approved` is exactly such a record, whether a
server's or a list's, and an entry in `mcp-projects` is a broader one, since it pre-answers a question about servers that do not
exist yet.

The mode is named in the session's own display for as long as it is in force, and the servers
reachable under it are listed rather than summarised.

**Why this clause exists when [permission-modes.md](permission-modes.md) already says it.** Because the list above is the part
nobody reads off those clauses. A mode that skips prompts is understood as a mode that turns the
safety off, and that reading is how it comes to be used in a container where it then does turn the
safety off. Almost nothing here is a prompt: confinement, the empty environment, the untrusted labels
and the declaration rule are properties of the code, and they are the ones that hold against a
hostile server. A prompt defends against a server a person would not have chosen. Skipping it decides
who is choosing; it decides nothing about what a server may do once chosen.

**A known cost.** In this mode a server declared and never seen by anybody runs on its first call.
That is the mode working as asked, and it is why the declaration file is the one file a session
cannot write: the mode removes the person from the loop, so the only remaining protection is that
nothing inside the workspace could have put a server there.

**A second known cost.** [SERVERS-8](#SERVERS-8)'s question is the one that puts a server's words in
front of the planner, so in this mode every tool's description and argument schema reaches the
planner endorsed by the mode rather than read by anybody. It is vouching's cost in
[MODE-4](permission-modes.md#MODE-4), reached through a server's list instead of a file. A list
offered this way stays offered after the session cycles out of the mode, because its words are
already in the context the planner writes from and taking the tools back would not take them out.
Each call from then on is asked.

**How it is built.** A requested server nothing approved is started without
[SERVERS-4](#SERVERS-4)'s question being drawn, and nothing is written to `mcp-approved` or
`mcp-projects`. A server's list is offered without [SERVERS-8](#SERVERS-8)'s question being drawn or
a check being made, since nobody would read the check's verdict, and no `tools` line is written. A
call is made without [SERVERS-7](#SERVERS-7)'s question being drawn, as answer 1 and never answer 2,
so nothing is written to `mcp-tools`, and out of the mode the next call is asked whatever was
offered in it. A hop off a remote server's declaration is refused without
[SERVERS-11](#SERVERS-11)'s question being drawn, as the call's or the handshake's, and nothing is
rewritten. A `deny` rule still refuses a call
([MODE-6](permission-modes.md#MODE-6)). Confinement, the named variables and the egress gate are the
same code in either mode. The display names the mode and the servers the session started. It does
not say beside each one whether it started unasked because of the mode.

`verified-by: bravebot_cli::servers::skipping_permissions_starts_the_server_unasked_and_records_nothing`
`verified-by: bravebot_agent::mcp::bypassing_answers_both_prompts_and_records_nothing`
`verified-by: bravebot_agent::mcp::a_list_offered_in_bypass_stays_offered_and_each_later_call_asks`
`verified-by: bravebot_agent::mcp::bypassing_refuses_a_hop_and_asks_nobody`
`verified-by: bravebot_agent::permission_mode::bypassing_refuses_to_move_a_server`
`verified-by: bravebot_cli::servers::a_redirected_handshake_nobody_is_asked_about_starts_nothing`

<a id="SERVERS-14"></a>
### SERVERS-14: what is reachable is visible without running anything

`bravebot mcp list` and `doctor` both answer, per alias: the transport, which file declared it,
whether it is approved and against which digest, whether a capability grants calls, which checkout
requested it, and which standing answers are recorded for it here. A declaration that is present and
unapproved is shown as present and unapproved rather than omitted.

The session's own display says how many servers are reachable in this session, and says none where
none are.

**Why.** The failure this closes is the one issue #83 describes in its other half, where an
interface reports a property that is not in force. A list that quietly omitted the unapproved would
make a declaration somebody wrote and never answered for look like a file that was never read.

`bravebot mcp list` says, per alias, the transport, whether it is approved, and the digest, under
the path of the file that declared it and the directory a session would start in, and, where the
managed layer refuses the server, the file that refused it and why ([SERVERS-12](#SERVERS-12)).

Under each row is the file here that requested it and whether a session started here holds the
grant to call it ([SERVERS-9](#SERVERS-9)). One it starts unasked does. One it asks about first
does after a yes. One it would not start does not, with the reason where the row does not already
give it. The answer comes from the same decision a session makes as it opens, so the list and the
session cannot disagree. A server nothing here requested holds no grant, and says so. The rows
answer for a session at a terminal: where nobody can be asked, one it would ask about is absent
([SERVERS-4](#SERVERS-4)), and one bypassing the prompts starts it unasked
([SERVERS-13](#SERVERS-13)).

Then come the standing answers recorded for it here: answer 2 at the server question, said under
each server the project requests whose declaration nobody saw as something else, since that is what
it covers ([SERVERS-5](#SERVERS-5)), and the tools answer 2 at a call covers. Where there are none,
the list says that nothing is answered. A server requested here and declared nowhere is a row of its
own. A declaration that cannot be used is listed with its problem, and the list then fails.

`doctor` prints the same rows after the state directory, under a heading that names the declarations
file and the directory. It fails where the list does, with the status for a failed check rather
than the one for a configuration nothing ran on, since a session here still opens without that
server. A machine with no state directory says so and does not fail, since no session there starts
a server. Neither prints the value of a variable a declaration names
([SERVERS-10](#SERVERS-10)), and a control character in the directory or a settings file's path is
shown as its picture, so a name cannot write a row of its own.

The session's display is built in the full-screen interface, whose `/status` has a line naming the
MCP servers the session started,
by alias, each saying how its tools stand: put to the person before the next turn plans, how many
are offered to the model, or none as the person answered. It says `none` where the session started
no server. A requested server that was not started is not on it: that is said once, as the
session opens, by the line giving the reason. Where a stdio server was started, the confinement
line says the level is in force over those servers and over nothing else the session runs, which is
the report [SANDBOX-10](sandboxing.md#SANDBOX-10) would otherwise make untrue.

`verified-by: bravebot_cli::running::an_added_server_nobody_was_asked_about_is_declared_and_listed_unapproved`
`verified-by: bravebot_cli::running::a_declaration_that_cannot_be_used_is_listed_with_its_problem`
`verified-by: bravebot_cli::mcp::list_says_who_requested_each_server_whether_a_session_here_holds_its_grant_and_what_stands`
`verified-by: bravebot_cli::mcp::list_says_a_requested_server_no_session_here_starts_holds_no_grant`
`verified-by: bravebot_cli::mcp::doctor_reports_what_list_does_and_fails_where_it_does`
`verified-by: bravebot_cli::running::list_and_doctor_in_a_checkout_say_what_requested_each_server_and_what_a_session_there_holds`
`verified-by: bravebot_tui::status::the_servers_a_session_started_are_named_and_what_is_confined_follows_them`
`verified-by: bravebot_tui::status::each_servers_note_says_how_its_tools_stand`

## Testing this with the weather server

The acceptance walk, end to end, with the server this spec was written against. Every prompt below
is one of the clauses above; every refusal is a clause refusing.

**1. Declare it.** A person types:

```
bravebot mcp add weather -- npx -y @dangahagan/weather-mcp@latest
```

The declaration lands in `~/.bravebot/mcp.json`, naming `PATH` for `npx`
([SERVERS-10](#SERVERS-10)). Nothing is reachable yet ([SERVERS-3](#SERVERS-3)).

**2. Answer the server question.** `add` asks [SERVERS-3](#SERVERS-3)'s question, naming `npx` as a
runner that fetches its own code and `@latest` as unpinned ([SERVERS-6](#SERVERS-6)). `y` records
the digest and requests the server in the current directory's `.bravebot/settings.local.json`, the
file `-s` names by default. Declining leaves it declared and requested nowhere, and
`bravebot mcp enable weather` asks again.

**3. Let a checkout ask for it.** To share the request with whoever clones the checkout, type:

```
bravebot mcp enable weather -s project
```

It puts `{ "mcp": { "request": ["weather"] } }` in `.bravebot/settings.json` and leaves the rest of
the file as it was. On the next session the alias resolves against the declaration and is
reachable. Change the request to an alias nobody declared and the session reports it and installs
nothing ([SERVERS-2](#SERVERS-2)). `bravebot mcp disable weather` takes the request out of each
file that holds it.

**4. Answer the list question, then the call question.** At the first turn,
[SERVERS-8](#SERVERS-8)'s prompt shows every tool the server lists as `weather:<tool>`, with its
arguments and every row of its description behind a margin. `1` offers them and records the list,
so the next session asks nothing about it. The planner asks for Toronto's conditions.
[SERVERS-7](#SERVERS-7)'s prompt shows `weather:get_current_conditions`, the arguments, and the
server's own description behind a margin, cut to two rows until `e` expands it. Answer 1 runs it
once. Answer 2 records the tool for this project, and `bravebot mcp forget` drops that record.

**5. What comes back.** Three calls, run against this server on 2026-09-20 at 19:45 local, as the
evidence that the wiring works:

| Call | Result |
|---|---|
| `weather:get_current_conditions` | Toronto, 43.6535 -79.3839. Overcast, 17 C, dewpoint 10 C, humidity 61%, wind 9 km/h from 339 degrees gusting 26, pressure 1018 hPa, cloud 100%. Source says NOAA does not cover this location and Open-Meteo model data is shown instead. |
| `weather:get_forecast`, 7 days | 20 Sep high 22 C, then 14, 17, 17, 17, 19, 24 C through 26 Sep. Precipitation chance 0 to 13% all week. Clear from the 25th. |
| `weather:get_alerts` | No active alerts. Source is Environment and Climate Change Canada, MSC GeoMet. |

The same call made by this program, from the planner through step 4's prompts, on 2026-09-25 at
16:15 Toronto time, came back as clear sky, 67 F, a high of 72 F and a low of 46 F, dewpoint 49 F,
with the same note that Open-Meteo data stands in for NOAA. The person sees it behind the margin.
The planner is handed a reference to it and the word that it is quarantined, and nothing of it.

Every one of those strings is untrusted content under [MCP-1](mcp.md#MCP-1): the numbers above are
what a person reading a marked result sees, not values this program has vouched for.

**6. Check the negatives.** Each of these must fail, and the clause that refuses it is the test:

| Do this | Expected |
|---|---|
| Put the argv in `.bravebot/settings.json` instead of declaring it | A parse error from `doctor`, no server. [SERVERS-1](#SERVERS-1) |
| Put the argv in `.bravebot/settings.local.json` | The same. Being uncommitted changes nothing. [SERVERS-1](#SERVERS-1) |
| Change `@latest` to a pinned version in the declaration | The server is unapproved and asks again, naming the change. [SERVERS-5](#SERVERS-5) |
| Run a one-shot with an unapproved server | The server is absent and the reason is printed. Nothing is approved. [SERVERS-4](#SERVERS-4) |
| Have the server report its tool as `write_file` | It is offered as `weather:write_file`, and a call to `write_file` is the built-in's. [SERVERS-8](#SERVERS-8) |
| Answer 2 at the list question | None of its tools is offered in this session, a call to one is told there is no such tool, nothing is recorded, and the next session asks again. [SERVERS-8](#SERVERS-8) |
| Run a one-shot with a list nobody vouched for | None of its tools is offered, and the reason is printed. [SERVERS-8](#SERVERS-8) |
| Run a one-shot where a call would ask | The call is refused and the planner is told the person did not approve it. Nothing is recorded. [SERVERS-7](#SERVERS-7) |
| Write `"permissions": { "deny": ["Mcp(weather:get_alerts)"] }` in the home settings | A call to it is refused before anybody is asked, and the others still ask. [SERVERS-7](#SERVERS-7) |
| Declare a stdio server on a machine where confinement is unavailable | Not launched. [MCP-3](mcp.md#MCP-3), in every mode. [SERVERS-13](#SERVERS-13) |
| Run in the skip-prompts mode, then run again without it | Every question is asked again; the first run recorded nothing. [SERVERS-13](#SERVERS-13) |
| Declare a remote server whose calls answer with a redirect to another host, and answer no when asked | Nothing is sent to that host, the planner is told the call was refused at the declared one, and `mcp.json` is unchanged. [SERVERS-11](#SERVERS-11) |
| The same in the skip-prompts mode | Nobody is asked, nothing is sent to that host, and nothing is rewritten. [SERVERS-13](#SERVERS-13) |
| Give the server a variable it needs without naming it in the declaration | It does not receive it. [SERVERS-10](#SERVERS-10) |
| Run `bravebot mcp add brave-search -e BRAVE_API_KEY_FILE=/path/to/key -- npx -y @brave/brave-search-mcp-server`, the server's own install line | Declared with the value stored and shown by its name, and the key file shown as one it may read. [SERVERS-10](#SERVERS-10) |
| Run `bravebot mcp add weather -e TOKEN sk-live -- npx weather-mcp` | Refused as word 3 after `add`, since a name alone is read only as the one word its `-e` takes, and the key is not repeated. [SERVERS-10](#SERVERS-10) |
| Write `{"mcp": {"deny": [{"command": ["/opt/weather-mcp", "--stdio"]}]}}` in the machine's `managed.json`, where `weather` runs that argv | The server is not started in any mode, whatever it is called, the session's line names that file and the entry, and `mcp list` says the same. [SERVERS-12](#SERVERS-12) |
| Write `{"mcp": {"allow": [{"host": "*.corp.example"}]}}` in the machine's `managed.json` | A server whose url names a host under `corp.example` starts as it did; every other server, local ones included, is not started, and the line says the allow list does not name it. [SERVERS-12](#SERVERS-12) |
| Write `{"mcp": {"weather": {...}}}` in the machine's `managed.json` for a server not declared at home | Nothing is declared, and a request for it still resolves to nothing. [SERVERS-12](#SERVERS-12) |

## Amendments to existing specs

This spec cannot land without these. Each is named by what the clause says rather than by its id.

| Spec | The clause | Change |
|---|---|---|
| [mcp.md](mcp.md) | front matter, `documented-by: none (internal: no settings key wires up a server yet, so there is nothing a reader can configure)` | Replaced. There is now something a reader configures, so the spec names the reference page and stops describing itself as internal. **Applied** with the declaration. |
| [mcp.md](mcp.md) | [MCP-9](mcp.md#MCP-9)'s known cost, that a server needing a variable does not work, "left to whatever names variables for a server when servers are reachable from something (issue #83)" | Discharged by [SERVERS-10](#SERVERS-10). The cost paragraph becomes a pointer rather than an open cost. **Applied** with the launch that hands the named variables over, and the clause now also says the launch hands over those and no others. |
| [mcp.md](mcp.md) | [MCP-1](mcp.md#MCP-1), what a server returns is untrusted | Extended, not amended. A description and an input schema join a result as content from outside, which is [SERVERS-8](#SERVERS-8). The clause's reasoning already covers them; the wording says "a tool result". |
| [layering.md](layering.md) | [LAYER-1](layering.md#LAYER-1)'s table, and `bravebot-mcp` being a crate nothing depends on | Amended. Whichever of `bravebot-agent` and `bravebot-cli` reaches the client gains the dependency, and the row's constraint gains the declaration surface. `a_rows_dependency_list_is_what_the_manifest_asks_for` fails until the table is updated, which is the check working. **Applied**: the `bravebot-cli` and `bravebot-config` constraints name the declaration surface, and `bravebot-cli` is the crate that reaches the client, so its row lists `mcp` and says it starts a server only on a person's answer, a recorded one, or the mode that answers for them. `bravebot-agent` reaches it too, to offer a started server's tools and call them, so its row lists `mcp` as well, and `bravebot-mcp` lists `config` for the one rule of what a tool's word may be. |
| [backends.md](backends.md) | [BACKEND-1](backends.md#BACKEND-1), a settings file may name a destination and never a permission, and none of them names a command to run | **Unchanged and reaffirmed.** [SERVERS-1](#SERVERS-1) exists so this clause does not have to move. A reviewer should read any future proposal to put a declaration in a settings layer as a proposal to amend this. |
| [backends.md](backends.md) | [BACKEND-38](backends.md#BACKEND-38), the managed file pins the names that decide where a request goes, and "every other name in the file decides nothing" | Extended by two names that are not destinations, `mcp.allow` and `mcp.deny`, which keep a server from starting and add none, for [SERVERS-12](#SERVERS-12)'s reason. **Applied**. |
| [backends.md](backends.md) | [BACKEND-24](backends.md#BACKEND-24), settings layers resolve a name at a time | Extended. `mcp.request` is a list, and joins the row where every layer's entries are kept, for that row's reason: an entry only ever names something that then has to be approved separately. **Applied** with the session that reads the key. |
| [permissions.md](permissions.md) | [PERM-1](permissions.md#PERM-1), a rule names a family of tools and matches on routing only, and "four families exist" | Amended. A fifth family, `Mcp`, with `Mcp(weather)` covering a server and `Mcp(weather:get_current_conditions)` one tool of it. The routing field is the alias and the tool name; the arguments are payload and no specifier matches them, which is [SERVERS-7](#SERVERS-7). **Applied**: `Mcp(weather:*)` is `Mcp(weather)`, and a name is matched whole, so `Mcp(weather)` does not cover `weather2`. |
| [permissions.md](permissions.md) | [PERM-9](permissions.md#PERM-9), three prompts no rule can answer | Extended to four. A call carrying the person's private data asks whatever the rules say, for that clause's own confidentiality reason. A server is further from the person than a local program is, not closer. **Applied** in the policy, and not reached: nothing labels a planner's arguments private yet. |
| [permission-modes.md](permission-modes.md) | [MODE-4](permission-modes.md#MODE-4)'s list of what bypassing answers | Extended by three prompts, a server, its list and a call, and by nothing else, and told of the fourth it refuses, whether a server moved. **Applied**. [SERVERS-13](#SERVERS-13) is the list of what the mode does not reach, which is [MODE-7](permission-modes.md#MODE-7) applied here. That spec's own rule, that nothing approved this way is recorded, covers the new records by its own argument. |
| [vetting.md](vetting.md) | [CHECK-10](vetting.md#CHECK-10), a check before every prompt that would promote quarantined content | Extended by a fourth prompt, a server's tool list, whose check reads the whole list as it is drawn. **Applied**. |
| [labels.md](labels.md) | [LABEL-8](labels.md#LABEL-8)'s roads in | Extended by two rows: a server's tool list a person vouched for, or the mode that answers for them, is trusted and public, and so is where a server's reply pointed once a person says it moved there. **Applied**. |
| [vetting.md](vetting.md) | the table of routes: `~/.bravebot/vetting` holding one word, and `"vetting": { "auto": true }` read from the home layer only | **Unchanged**, and the precedent this spec's storage copies rather than a spec to amend. A standing answer in the person's own directory, and the key that changes behaviour readable from one layer, are both already settled there. |

## What is deliberately not adopted

- **A declaration in any settings layer.** [SERVERS-1](#SERVERS-1). This is the whole of the security
  argument and the one change that would void most of this spec.
- **Installing a server to satisfy a request.** A checkout naming an alias nobody declared gets a
  report. Fetching a package because a file in the workspace asked is the hole
  [SERVERS-1](#SERVERS-1) closes, arriving by a different road.
- **A standing answer stored inside the checkout.** The behaviour is adopted, the location is not.
  [The divergence](#the-one-place-this-diverges-from-claude-code) says why.
- **A standing answer that reaches every project.** Answer 2 at either prompt is bounded to the
  project it was given in. A switch meaning "everywhere" is the skip-prompts mode, and it is named
  as what it is rather than accumulated one answer at a time.
- **A shell string as the declaration.** argv after a bare `--`. Nothing here starts an interpreter.
- **The confinement display half of issue #83.** That a session reports a confinement level while
  nothing in it is confined is separable and cheaper, and the issue says so. It should land first and
  on its own, because this spec makes the report true rather than fixing the report.
- **Confinement of a remote server.** There is no process. [SERVERS-11](#SERVERS-11) is what stands
  in its place, and it is a weaker thing.

## Open questions

- **Whether `remove` should take `-s`.** Claude Code's `remove -s project` takes a server out of the
  project's file. Here the declaration is in one file only, and what a project's file holds is a
  request, which `disable -s project` takes out, so a `remove -s` would be a second spelling of
  `disable` or a `remove` that leaves the request in place.
- **Whether answer 2 at the server prompt should survive a new declaration in the same project.** As
  written it does: a project path is recorded, and a server declared later is reachable there without
  asking. That is what the Claude Code wording promises and it is the point of the answer. It also
  means the second server is never seen by anybody, which is the one case where this is the weaker
  of the two gates.
- **Whether a tool's arguments should ever bound a standing answer.** [SERVERS-7](#SERVERS-7) says
  no, on the grounds that no two calls share arguments. A prefix, the way a permission rule matches a
  command, is the obvious middle and it needs a spelling that a tool's own schema does not fight.
- **Whether the digest should cover the resolved program rather than the name.** `npx` resolved
  through `PATH` is a different file on two machines, and [SERVERS-10](#SERVERS-10) makes `PATH` a
  thing the declaration names. Digesting the resolved path would make an approval machine-specific,
  which is either the correct reading of what was approved or a re-approval every time a toolchain
  moves.

## Known costs

- **A settings file `mcp` writes is rewritten whole.** `enable`, `disable` and an `add` that requests
  keep every value, and not the file's spacing or key order, for the reason
  [import.md](import.md)'s known cost gives.
- **A checkout's link is not written through.** Where `.bravebot` in a checkout, or its settings
  file, is a link, `enable` and `disable` refuse it, and the request is written by hand in the file
  it leads to.
- **A file `mcp` writes in a checkout is the person's own.** On Unix it is written readable by its
  owner alone, as the state directory's files are, an existing one included, and a `.bravebot` it
  creates there is too, so a file other accounts on the machine read is set readable again by hand.
- **Nothing keeps `settings.local.json` out of a commit.** It is the per-person file only by name,
  and the checkout's own ignore file is what keeps it there.
- **An approval is a channel, not code.** [SERVERS-6](#SERVERS-6) says so at the prompt and cannot do
  better. For the distribution form nearly every server uses, the digest binds the argv and the argv
  names a package whose contents change. The only real fix is pinning with an integrity hash, which
  the package ecosystems support and this declaration format does not yet express.
- **A person who approves a server has approved a program.** Beyond the confinement
  [MCP-3](mcp.md#MCP-3) requires and the capability [SERVERS-9](#SERVERS-9) grants, there is no audit
  of what it does once it runs. A confined process with a network capability is still a program
  reading a workspace.
- **Three prompts is three prompts.** A server gate and a call gate are the shape asked for, and a
  list gate is what letting a server's words reach the planner costs. Together they are more asking
  than the tool this is measured against does, because the call gate here has no per-server "never
  ask again". The standing answers are what pay it back. A list is asked about again only when it
  changes, and a person who answers 2 at the other two has the same quiet session with a record of
  having been asked.
- **The useful case costs two steps.** A newcomer to a checkout that requests a server has to declare
  it themselves and then approve it. That is the price of a shared file that cannot hand anything
  over, and it is paid by the person who wanted the server.
- **A marked description is still in the context.** [SERVERS-8](#SERVERS-8) keeps a server's words out
  of the instructions and inside a margin, and a planner that reads them has still read text somebody
  else wrote, which a person read before saying yes. The margin is a boundary for a person looking at a screen; for a model it is a
  convention, and conventions are what injection attacks are made of.
- **The standing answers do not travel.** Recording them outside the checkout means a second machine
  asks again and a wiped workspace does not forget. `mcp forget` is the deliberate road; there is no
  accidental one.
- **The plain interface cannot stop asking.** Its call question is a line answered yes or no, and
  its yes is answer 1, so answer 2 is given in the full-screen interface or not at all.
- **A run nobody can be asked in still checks a list.** A one-shot run makes
  [CHECK-10](vetting.md#CHECK-10)'s check of a list nobody vouched for, then refuses the list.
  Nothing tells the turn that its questions will be refused rather than asked, so the check is a
  model call whose word nobody reads.
- **A call carrying private data is not told apart yet.** The policy asks about one whatever is
  standing, and nothing labels a planner's arguments private, so a standing answer or an `allow`
  rule answers every call today ([SERVERS-7](#SERVERS-7)).
- **The full-screen interface asks before it asks about the directory.** The server question is put
  on the plain terminal before that interface takes the screen, and the question about trusting the
  directory is asked inside it. A server can therefore start for a session whose directory is then
  declined, and it runs until the process exits.
- **Nothing removes a server's old home.** A declaration that changes leaves the directory the one
  before it wrote under `~/.bravebot/mcp-home`, a runner's whole cache among it, until somebody
  removes it.
- **A toolchain is found through its own installation and nowhere else in the home directory.** A
  runner whose program loads from somewhere other than the installation it came from, such as a
  version manager's shim that hands over to a program elsewhere in the home, is not given that
  somewhere, and does not start confined.
- **The full-screen interface discards a server's stderr.** It owns the screen, and a server's
  diagnostics drawn over it would be a server's bytes where the interface draws. A one-shot run and
  the plain interface pass it through to their own stderr.
- **A server too slow for its handshake is left running.** A server that has not answered within 60
  seconds is left out of the session, and the thread waiting for it holds it until it answers or the
  process exits. The session opens after the last handshake or those 60 seconds, and draws nothing
  while it waits.
- **A server's line is read whole.** The client reads each line a server writes to its end before
  looking at it, with no bound, so a started server that writes one line without end holds this
  process's memory while it does.
- **No stdio server starts on Windows.** The sandbox there has no base rows to build a server's
  policy on, so the line says the platform has no confinement for one yet, which is
  [MCP-3](mcp.md#MCP-3) holding rather than failing.
- **Only the directories above a server's directory are searched for a repository.** A directory
  holding checkouts is not refused, so the server may write their work trees, a hook a relative
  `core.hooksPath` points at among them, on every platform.
- **On Linux a server may still write a repository it can reach.** Landlock grants a directory with
  everything beneath it and cannot hold one subdirectory back, so a `.git` beneath the server's
  directory, its own home or the temporary directory is one it may write, whether it was there
  before the server started or the server made it, and git run there later runs what the server
  wrote. On every platform a directory a server lays out as a repository itself, with no `.git` in
  its path, is not refused once the server is running.
- **On macOS a server cannot make a work tree.** No `.git` is writable to it, so a server or a
  runner that clones a repository into its home or the temporary directory fails there, and on
  every platform a server meant to work on a git repository cannot be given one.
- **A person whose home directory is a git work tree** has every directory in it refused as a
  server's, on every platform.
- **An administrator's deny list names a spelling.** A command entry binds a path and a host entry
  a name, so a link, a copy or another name for the same machine is not denied, and a command match
  leaves out the variables a server starts with. The allow list is the form that holds
  ([SERVERS-12](#SERVERS-12)).
- **The desktop application starts no server.** It has none of the three questions a server is put
  through: the server question ([SERVERS-4](#SERVERS-4)), the tool list
  ([SERVERS-8](#SERVERS-8)) and the call ([SERVERS-7](#SERVERS-7)). It answers each of them no, so
  a server it started would be one nobody was asked about. A desktop session reads the same
  settings file as the terminal's three sessions, starts nothing for a request, and names what was
  requested as it opens ([SERVERS-2](#SERVERS-2)), so a person who needs the servers knows to start
  the session in a terminal.
