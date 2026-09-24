---
id: SERVERS
title: Declaring an MCP server
status: proposed
governs:
  - crates/mcp/src/lib.rs
  - crates/mcp/src/protocol.rs
  - crates/mcp/src/stdio.rs
  - crates/mcp/src/http.rs
  - crates/config/src/settings.rs
  - crates/core/src/capability.rs
  - crates/core/src/permissions.rs
  - crates/core/src/policy.rs
  - crates/agent/src/tools.rs
  - crates/tui/src/confirm.rs
documented-by: none (unbuilt: a reader can configure nothing until a crate reads a declaration, so there is no page to write yet. The reference page lands with the work, and this line is what says it is missing)
---

## Scope

How a person says that a server exists, what that statement is trusted for, where the statement is
kept, and what is asked before a tool from one runs. The protocol itself, what a result is labelled
and why a stdio server is confined, is [mcp.md](mcp.md) and is unchanged here. This spec is the
surface that spec has none of.

## What exists today

Almost nothing in this spec is built, and what there is amounts to half of three clauses.
[SERVERS-9](#SERVERS-9)'s capability names the server it is about, so the gate `crates/mcp/`
already runs on every call asks about one declared server rather than about the protocol, and a
grant can be withdrawn while the run is going. What would *put* a grant there is still missing, so
a caller writing the set out itself is the only thing that grants a call today.
[SERVERS-11](#SERVERS-11)'s gate holds a request to a remote server to the destination it was
addressed to, and refuses a hop that leaves it. The question that hop is meant to raise is missing
for the same reason, since there is no declaration to ask about or to write an answer into.
[SERVERS-8](#SERVERS-8)'s namespace is composed by the client rather than reported by the server,
and the sentence and the schema a server sends about a tool arrive labelled. What is missing there
is the other end: no tool surface is assembled from a server's list, so nothing yet draws the
margin those labels ask for.

`crates/mcp/` is a finished client under nine normative clauses, and no crate depends on it: `mcp.md` records the gap in its own front matter, `documented-by: none
(internal: no settings key wires up a server yet, so there is nothing a reader can configure)`.
Issue #83 is where that was written down, and it names the four things wiring needs decided first:
where a server is declared, what a name and an argv is trusted for, how the untrusted label
[mcp.md](mcp.md) puts on a result reaches a slot, and whether servers are confined. Every clause
below answers one of them.

Read every clause here as a requirement on work nobody has started, not as a description of this
program. The `verified-by: none` on each one but [SERVERS-8](#SERVERS-8),
[SERVERS-9](#SERVERS-9) and [SERVERS-11](#SERVERS-11) is the honest form of that, and is what keeps
a reader from taking the present tense as a claim about the current build. Those three are the
exceptions because the half each pins is about what the client does rather than about a
declaration, and the client exists: what it does is pinned, and nothing calls it.

## The parity target

What Claude Code offers, as the list this spec is measured against. Two rows in bold are where
parity is refused on purpose. One row in italics is where the behaviour is adopted and the storage
is not, and [the divergence](#the-one-place-this-diverges-from-claude-code) says why.

| | Claude Code | This spec |
|---|---|---|
| Declaration file | `.mcp.json` at the repo root, or user scope | `~/.bravebot/mcp.json`, the person's own directory only |
| A file in the checkout may declare a server | yes, argv included | **no**, an alias may be requested and nothing more |
| Scopes | local, project, user | home declares; a checkout requests; a machine-level layer removes |
| Prompt when a server is first reachable | three answers: use it, use it and all future servers in this project, continue without it | same three answers, same meanings |
| Prompt on every tool call | three answers: yes, yes and stop asking for this tool here, no | same three answers, same meanings |
| *Where "stop asking" is recorded* | *`.claude/settings.local.json`, inside the checkout* | *the state directory, keyed by the project path* |
| Adding one from the command line | `claude mcp add` | `bravebot mcp add`, and the typing is not the approval |
| Re-approval when the declaration changes | not on argv change | **yes**, an approval binds to a digest |
| Environment reaching the server | the whole process environment, plus `env` | the named variables and nothing else |
| Unpinned command (`npx -y pkg@latest`) | run as written, silently | named at the prompt as code that differs per run |
| Tool names | as the server reports them | namespaced by the local alias; a server's own words are never an identifier |
| Tool descriptions | into the planner's context as trusted text | untrusted content, and marked |
| Confinement of a stdio server | none | required already by [MCP-3](mcp.md#MCP-3) |
| A remote server | fetched directly | an egress destination, and the host is approved |
| A mode that skips the prompts | `--dangerously-skip-permissions` | [SERVERS-13](#SERVERS-13), and it skips prompts only |

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
bravebot mcp add <alias> --stdio -- <program> [args...]
bravebot mcp add <alias> --http <url>
bravebot mcp get <alias>
bravebot mcp list
bravebot mcp approve <alias>
bravebot mcp remove <alias>
bravebot mcp forget [path]
```

`add` writes a declaration and then asks. `approve` is the same question for a declaration that is
already written, which is what a checkout's request and a changed digest both produce. `get` shows
one declaration in full, including its digest and the variables it will receive. `list` says, per
alias, the transport, whether it is approved, whether a capability grants calls to it, and whether
anything requested it. `remove` deletes the declaration and its approval together, because an
approval outliving its declaration is a digest nothing resolves. `forget` drops the standing answers
recorded for a project, defaulting to this one.

`--stdio -- ...` takes the program and its arguments after a bare `--`, as argv and never as a line.
Nothing here compiles shell syntax: that is [tools/command-line.md](tools/command-line.md)'s road,
for a line a planner wrote, and a server is a program a person named.

### Coming from Claude Code

| Claude Code | Here | Note |
|---|---|---|
| `claude mcp add weather -- npx -y @dangahagan/weather-mcp@latest` | `bravebot mcp add weather --stdio -- npx -y @dangahagan/weather-mcp@latest` | The transport is named rather than inferred from whether a url was given. |
| `claude mcp add weather -s user -- ...` | `bravebot mcp add weather --stdio -- ...` | Already the only scope that may declare, so the flag would have one value. |
| `claude mcp add weather -s project -- ...` | no equivalent | [SERVERS-1](#SERVERS-1). The checkout may request the alias; it may not carry the argv. |
| `claude mcp get weather` | `bravebot mcp get weather` | Also prints the digest, which is what an approval is against. |
| `claude mcp remove weather -s project` | edit `"mcp": { "request": [...] }` in `.bravebot/settings.json` | A request is a line in a file somebody commits, so it is removed the way it was added. |
| `claude mcp list` | `bravebot mcp list` | Unapproved declarations are listed as unapproved rather than omitted. |

## Where things are stored

| What | Where | Scope | Lifetime |
|---|---|---|---|
| A declaration: alias, transport, argv or url, the variable names it needs, its directory | `~/.bravebot/mcp.json` | the person's own directory | until they change it |
| An approval of a server, one digest per line | `~/.bravebot/mcp-approved` | the person's own directory | until the declaration changes |
| "use all future servers in this project", one project path per line | `~/.bravebot/mcp-projects` | the person's own directory | until `mcp forget` |
| "stop asking for this tool here", one alias, tool and project path per line | `~/.bravebot/mcp-tools` | the person's own directory | until `mcp forget` |
| A request for an alias, `"mcp": { "request": ["weather"] }` | `.bravebot/settings.json` beside the work | that checkout | that checkout |
| A removal, `"mcp": { "deny": ["weather"] }` | the managed layer | the machine | as long as it is pinned |
| A declaration, an argv, a url, or a variable's value | `.bravebot/settings.json` or `.bravebot/settings.local.json` | **nothing.** [SERVERS-1](#SERVERS-1) | n/a |
| A variable's value | nowhere. The declaration names variables; the values are the person's own environment at launch | n/a | n/a |

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
needs, and the directory it runs in is `~/.bravebot/mcp.json`. A settings layer carries none of
those, at any of the three levels, including the one the command line names.

A project layer that names an argv is a parse error reported by `doctor`, not a declaration that is
quietly ignored, because a person who wrote one believes it works.

**Why.** [BACKEND-1](backends.md#BACKEND-1) forbids a settings file from naming a command to run,
and the agent's own write gate covers the workspace, so any in-checkout file that could name one
turns a tool call into execution nobody approved. Keeping the declaration outside every layer also
keeps `doctor`'s answer about settings free of a question it cannot settle.

**Unbuilt, so nothing pins this.** No crate reads a declaration, so there is nothing yet to hold to this.

`verified-by: none`

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

**Unbuilt, so nothing pins this.** The request key is not read, and no resolution against a declaration exists.

`verified-by: none`

<a id="SERVERS-3"></a>
### SERVERS-3: adding a server is a command a person types, and typing it is not the approval

`bravebot mcp add` writes the declaration and then asks [SERVERS-4](#SERVERS-4)'s question. The
answer to that question is what grants anything.

**Why.** A person typing a command line is the strongest endorsement this program has, and it is
still not an endorsement of what the line will do on the tenth run. Splitting the two means the
record of what was approved is a record of a thing somebody read, and it is what makes the
re-approval in [SERVERS-5](#SERVERS-5) meaningful rather than a formality.

**Unbuilt, so nothing pins this.** There is no mcp subcommand.

`verified-by: none`

<a id="SERVERS-4"></a>
### SERVERS-4: a server reachable for the first time is put to the person, with three answers

Before any tool from a server is offered to the planner, the person is shown the alias, the
transport, the resolved program and its argv with argument boundaries visible, or the url and its
host, the variable names it will receive, the directory it will run in, the digest, and which
checkout requested it where one did.

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
pre-answer [SERVERS-7](#SERVERS-7)'s question about a call, and it does not reach a project other
than the one it was given in.

Where nobody can be asked, the server is absent and the reason is said, which is
[LAYER-1](layering.md#LAYER-1)'s constraint on the CLI crate: where nobody can be asked, effects are
refused rather than applied unseen. A one-shot run, a run with no terminal, and a delegate all take
that road. None of them approves a server.

**Why three answers rather than two.** The middle one is the answer people actually want in a
checkout they trust, and its absence is what drives them to find a switch that turns the asking off
for everything everywhere. Bounded to one project it is a much smaller claim than that switch, and
it is the claim they meant.

**Unbuilt, so nothing pins this.** No server is reachable, so nothing is put to anybody.

`verified-by: none`

<a id="SERVERS-5"></a>
### SERVERS-5: an approval binds to a digest of the declaration

The digest covers the transport, the program and every argument, the url and its host, the names of
the variables passed in, and the directory. It does not cover a variable's value, which is not in
the declaration.

Changing any of those produces a digest nothing approved, so the server is unapproved until
[SERVERS-4](#SERVERS-4)'s question is answered again, and the question names what changed. A project
path recorded by answer 2 does not answer for a changed digest: it reaches a server this person has
not seen, and a changed argv is a server they have not seen.

**Why.** Same reason [CMDLINE-3](tools/command-line.md#CMDLINE-3) binds an endorsement to the
compiled plan rather than the line: the thing a person read has to be the thing that runs, and an
approval keyed on an alias would let an edit to the argv inherit the answer given about a different
program. An alias is a label a person chose; it is not what the label points at.

**Unbuilt, so nothing pins this.** Nothing computes a declaration digest and no approval record exists.

`verified-by: none`

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

**Unbuilt, so nothing pins this.** No prompt exists, so nothing classifies a program as a runner.

`verified-by: none`

<a id="SERVERS-7"></a>
### SERVERS-7: every call to a server's tool is put to the person, with three answers

A call is shown as the alias, the tool beneath it, and the arguments as the planner wrote them. The
server's own description of the tool is shown as content, marked, collapsed to a line with a way to
expand it, per [SERVERS-8](#SERVERS-8).

```
  weather:get_current_conditions        (MCP)

  city_name: "Toronto, Ontario, Canada"
  units:     "metric"

│ Get the most recent weather observation for a location. Use this for
│ current weather or when asking about "today's weather" ...        (expand)

  Proceed?
❯ 1. Yes
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

**Unbuilt, so nothing pins this.** No tool from a server reaches the planner, so no call is put to anybody.

`verified-by: none`

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

**Half built.** Listing a server's tools returns what this process may say about each one: the name
is the alias the server was reached by with the server's word beneath it, composed by the client, and
the server's word is kept only for the request that goes back to that server. The description and
the input schema come back labelled, on the same footing as a result. What is not built is the far
end: no tool surface is assembled from a server's list, so nothing marks a description as content
inside a margin, and no planner is offered a name from here.

`verified-by: bravebot_mcp::protocol::a_tool_is_named_by_the_alias_and_not_by_the_word_the_server_picked`
`verified-by: bravebot_mcp::protocol::the_same_word_from_two_servers_is_two_tools`
`verified-by: bravebot_mcp::protocol::the_servers_word_is_kept_for_the_request_and_is_not_the_name`
`verified-by: bravebot_mcp::protocol::a_tools_description_and_schema_are_content`
`verified-by: bravebot_mcp::stdio::a_confined_server_completes_the_handshake_and_lists_tools`
`verified-by: bravebot_mcp::http::a_handshake_and_tool_list_round_trip`

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

**Half built.** The capability names the alias, both transports gate on the one naming the server
in front of them, and a grant can be withdrawn while the run is going. What is not built is
anything that would put a grant there: no declaration exists for an alias to resolve against
([SERVERS-1](#SERVERS-1)) and nobody is asked ([SERVERS-4](#SERVERS-4)), so a caller writing the
set out itself is the only thing that grants a call today.

`verified-by: bravebot_core::capability::a_grant_for_one_server_is_not_a_grant_for_another`
`verified-by: bravebot_core::capability::withdrawing_one_grant_leaves_the_others`
`verified-by: bravebot_mcp::stdio::a_grant_for_one_server_does_not_reach_another`
`verified-by: bravebot_mcp::http::a_grant_for_one_server_does_not_reach_another`
`verified-by: bravebot_mcp::stdio::a_grant_withdrawn_stops_the_next_call`

<a id="SERVERS-10"></a>
### SERVERS-10: a variable a server needs is named in the declaration, and reaches that server alone

[MCP-9](mcp.md#MCP-9) empties the environment before a stdio server starts, on every platform, and
its known cost is that a server needing a variable to work at all does not work. This is that cost
paid: the declaration lists variable **names**, those names are read from this process's environment
at launch, and the resulting environment of the server is those values and nothing else.

No value is stored. A settings layer supplies none of them, and neither does the declaration: a
declaration that wrote a value in place of a name is a parse error. `PATH` is a name like any other,
so a server whose program must be resolved says so.

The names appear at [SERVERS-4](#SERVERS-4)'s prompt. The values appear in no prompt, no log, no
session record, and no `doctor` output.

**Why.** Naming a variable rather than holding a secret is the shape
[BACKEND-1](backends.md#BACKEND-1) already settled for a gateway credential, for the reason that a
file naming a destination grants nothing while a file holding a token is a token. Passing only the
named ones keeps [MCP-9](mcp.md#MCP-9)'s property where it matters: this process's own credentials
are in variables, and a server is code we did not write.

**Unbuilt, so nothing pins this.** A declaration cannot name a variable, so the cost
[mcp.md](mcp.md) records against its empty-environment clause stands unpaid.

`verified-by: none`

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

**The declaration is unbuilt, so the question cannot be asked.** No settings key declares a server,
so there is nothing to digest, no alias an edit could repoint, and nothing to rewrite an approval
into. The prompt is unbuilt with it: a hop is detected at the egress gate, which allows or refuses
and cannot ask, so an approval has to be a grant minted before the call and there is no prompt to
mint one. Until issue #83 declares a server, a hop that leaves the declared destination is refused
and nothing is sent, which is this clause with its question unasked rather than a different rule.
What is built reads a destination as a host and a port together, as above. Nothing calls it, since
no crate depends on the client.

`verified-by: bravebot_mcp::http::mcp_traffic_passes_through_the_network_gate`
`verified-by: bravebot_mcp::http::a_redirect_to_another_host_is_refused`
`verified-by: bravebot_mcp::http::a_redirect_within_the_declared_host_is_followed`
`verified-by: bravebot_mcp::http::a_failed_server_request_stops_confining_the_turns_other_egress`
`verified-by: bravebot_core::policy::a_rule_does_not_let_a_servers_request_be_redirected_off_its_host`
`verified-by: bravebot_core::policy::a_servers_request_cannot_be_redirected_to_another_port_on_the_same_host`

<a id="SERVERS-12"></a>
### SERVERS-12: the managed layer may remove a server and never add one

An administrator's layer may deny an alias, or deny all of them. It may not declare one, approve
one, grant a capability to one, or pre-answer either prompt.

**Why.** The machine-level layer exists to make an approved destination the only destination, and
the names it may pin are the ones that decide where a request goes. A layer that could add a server
would be a layer that installs a program on every machine it reaches, which is a much larger power
than the inversion that layer was built for, and `managed.rs` already argues the general form: a
layer that can pin anything is a layer somebody uses to pin a preference.

**Unbuilt, so nothing pins this.** The managed layer knows nothing about servers.

`verified-by: none`

<a id="SERVERS-13"></a>
### SERVERS-13: bypassing answers these two prompts and reaches nothing else here

[MODE-4](permission-modes.md#MODE-4) already decides the first half: `--dangerously-skip-permissions`
answers every permission question, and [SERVERS-4](#SERVERS-4) and [SERVERS-7](#SERVERS-7) are
permission questions, so both are answered yes without being drawn. This clause does not grant that
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
| [MCP-9](mcp.md#MCP-9) and [SERVERS-10](#SERVERS-10), the environment is the named variables and nothing else | The prompt showed the names. Skipping the showing does not widen the set. |
| [MCP-1](mcp.md#MCP-1) and [SERVERS-8](#SERVERS-8), a result, a description and a schema are untrusted | A label is not an approval. Nothing a person could have said would have made a server's output trusted, so there is nothing here for a skipped question to have granted. |
| [SERVERS-1](#SERVERS-1), only the person's own directory declares a server | An undeclared server does not become declared by nobody being asked about it. A checkout's request still resolves against declarations or resolves to nothing. |
| [SERVERS-9](#SERVERS-9), the capability | A capability is configuration, not a prompt. A server with no grant is called by nobody in this mode either. |
| [SERVERS-12](#SERVERS-12), a managed denial | An administrator's removal is not a question being put to the person running the program. |
| [SERVERS-11](#SERVERS-11), the egress gate and the host in the digest | The gate decides on labels. This mode answers two named questions and not every question, and a hop leaving the declared destination is neither of them, so it is refused here rather than followed. |

**Nothing is recorded.** A skipped question leaves no approval, no project path and no tool entry
behind, so a later run outside the mode asks every question as though this one had not happened. That
is not a new rule: [MODE-4](permission-modes.md#MODE-4) already says a run approved this way vouches
for no program, and that a record claiming somebody approved programs they were never shown would be
a standing permission nobody granted. A digest in `mcp-approved` is exactly such a record, and an
entry in `mcp-projects` is a broader one, since it pre-answers a question about servers that do not
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

**Unbuilt, so nothing pins this.** There is no mode here to skip, because there is nothing to ask.

`verified-by: none`

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

**Unbuilt, so nothing pins this.** There is nothing to list.

`verified-by: none`

## Testing this with the weather server

The acceptance walk, end to end, with the server this spec was written against. Every prompt below
is one of the two clauses above; every refusal is a clause refusing.

**1. Declare it.** A person types:

```
bravebot mcp add weather --stdio -- npx -y @dangahagan/weather-mcp@latest
```

The declaration lands in `~/.bravebot/mcp.json`. Nothing is reachable yet
([SERVERS-3](#SERVERS-3)).

**2. Answer the server question.** [SERVERS-4](#SERVERS-4)'s prompt appears, naming `npx` as a
runner that fetches its own code and `@latest` as unpinned ([SERVERS-6](#SERVERS-6)). Answer 1
records the digest.

**3. Let a checkout ask for it.** Put this in `.bravebot/settings.json`:

```json
{ "mcp": { "request": ["weather"] } }
```

On the next session the alias resolves against the declaration and is reachable. Change the request
to an alias nobody declared and the session reports it and installs nothing
([SERVERS-2](#SERVERS-2)).

**4. Answer the call question.** The planner asks for Toronto's conditions.
[SERVERS-7](#SERVERS-7)'s prompt shows `weather:get_current_conditions`, the arguments, and the
server's own description behind a margin ([SERVERS-8](#SERVERS-8)). Answer 1 runs it once; answer 2
records the tool for this project.

**5. What comes back.** Three calls, run against this server on 2026-09-20 at 19:45 local, as the
evidence that the wiring works:

| Call | Result |
|---|---|
| `weather:get_current_conditions` | Toronto, 43.6535 -79.3839. Overcast, 17 C, dewpoint 10 C, humidity 61%, wind 9 km/h from 339 degrees gusting 26, pressure 1018 hPa, cloud 100%. Source says NOAA does not cover this location and Open-Meteo model data is shown instead. |
| `weather:get_forecast`, 7 days | 20 Sep high 22 C, then 14, 17, 17, 17, 19, 24 C through 26 Sep. Precipitation chance 0 to 13% all week. Clear from the 25th. |
| `weather:get_alerts` | No active alerts. Source is Environment and Climate Change Canada, MSC GeoMet. |

Every one of those strings is untrusted content under [MCP-1](mcp.md#MCP-1): the numbers above are
what a person reading a marked result sees, not values this program has vouched for.

**6. Check the negatives.** Each of these must fail, and the clause that refuses it is the test:

| Do this | Expected |
|---|---|
| Put the argv in `.bravebot/settings.json` instead of declaring it | A parse error from `doctor`, no server. [SERVERS-1](#SERVERS-1) |
| Put the argv in `.bravebot/settings.local.json` | The same. Being uncommitted changes nothing. [SERVERS-1](#SERVERS-1) |
| Change `@latest` to a pinned version in the declaration | The server is unapproved and asks again, naming the change. [SERVERS-5](#SERVERS-5) |
| Run a one-shot with an unapproved server | The server is absent and the reason is printed. Nothing is approved. [SERVERS-4](#SERVERS-4) |
| Have the server report its tool as `write_file` | It is offered as `weather:write_file` and shadows nothing. [SERVERS-8](#SERVERS-8) |
| Declare a stdio server on a machine where confinement is unavailable | Not launched. [MCP-3](mcp.md#MCP-3), in every mode. [SERVERS-13](#SERVERS-13) |
| Run in the skip-prompts mode, then run again without it | Every question is asked again; the first run recorded nothing. [SERVERS-13](#SERVERS-13) |
| Give the server a variable it needs without naming it in the declaration | It does not receive it. [SERVERS-10](#SERVERS-10) |

## Amendments to existing specs

This spec cannot land without these. Each is named by what the clause says rather than by its id.

| Spec | The clause | Change |
|---|---|---|
| [mcp.md](mcp.md) | front matter, `documented-by: none (internal: no settings key wires up a server yet, so there is nothing a reader can configure)` | Replaced. There is now something a reader configures, so the spec names the reference page and stops describing itself as internal. |
| [mcp.md](mcp.md) | [MCP-9](mcp.md#MCP-9)'s known cost, that a server needing a variable does not work, "left to whatever names variables for a server when servers are reachable from something (issue #83)" | Discharged by [SERVERS-10](#SERVERS-10). The cost paragraph becomes a pointer rather than an open cost. |
| [mcp.md](mcp.md) | [MCP-1](mcp.md#MCP-1), what a server returns is untrusted | Extended, not amended. A description and an input schema join a result as content from outside, which is [SERVERS-8](#SERVERS-8). The clause's reasoning already covers them; the wording says "a tool result". |
| [layering.md](layering.md) | [LAYER-1](layering.md#LAYER-1)'s table, and `bravebot-mcp` being a crate nothing depends on | Amended. Whichever of `bravebot-agent` and `bravebot-cli` reaches the client gains the dependency, and the row's constraint gains the declaration surface. `a_rows_dependency_list_is_what_the_manifest_asks_for` fails until the table is updated, which is the check working. |
| [backends.md](backends.md) | [BACKEND-1](backends.md#BACKEND-1), a settings file may name a destination and never a permission, and none of them names a command to run | **Unchanged and reaffirmed.** [SERVERS-1](#SERVERS-1) exists so this clause does not have to move. A reviewer should read any future proposal to put a declaration in a settings layer as a proposal to amend this. |
| [backends.md](backends.md) | [BACKEND-24](backends.md#BACKEND-24), settings layers resolve a name at a time | Extended. `mcp.request` is a list, and joins the row where every layer's entries are kept, for that row's reason: an entry only ever names something that then has to be approved separately. |
| [permissions.md](permissions.md) | [PERM-1](permissions.md#PERM-1), a rule names a family of tools and matches on routing only, and "four families exist" | Amended. A fifth family, `Mcp`, with `Mcp(weather)` covering a server and `Mcp(weather:get_current_conditions)` one tool of it. The routing field is the alias and the tool name; the arguments are payload and no specifier matches them, which is [SERVERS-7](#SERVERS-7). |
| [permissions.md](permissions.md) | [PERM-9](permissions.md#PERM-9), three prompts no rule can answer | Extended to four. A call carrying the person's private data asks whatever the rules say, for that clause's own confidentiality reason. A server is further from the person than a local program is, not closer. |
| [permission-modes.md](permission-modes.md) | [MODE-4](permission-modes.md#MODE-4)'s list of what bypassing answers | Extended by two prompts, and by nothing else. [SERVERS-13](#SERVERS-13) is the list of what the mode does not reach, which is [MODE-7](permission-modes.md#MODE-7) applied here. That spec's own rule, that nothing approved this way is recorded, covers the two new records by its own argument. |
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

- **Whether answer 2 at the server prompt should survive a new declaration in the same project.** As
  written it does: a project path is recorded, and a server declared later is reachable there without
  asking. That is what the Claude Code wording promises and it is the point of the answer. It also
  means the second server is never seen by anybody, which is the one case where this is the weaker
  of the two gates.
- **Whether a tool's arguments should ever bound a standing answer.** [SERVERS-7](#SERVERS-7) says
  no, on the grounds that no two calls share arguments. A prefix, the way a permission rule matches a
  command, is the obvious middle and it needs a spelling that a tool's own schema does not fight.
- **Whether a description should reach the planner at all.** [SERVERS-8](#SERVERS-8) marks it.
  Withholding it entirely leaves the planner a schema and no statement of purpose, which is close to
  unusable; marking it means untrusted text is in the context on purpose. The third road is that a
  person writes the description when they approve the server, and the server's own is shown only to
  them.
- **Whether the digest should cover the resolved program rather than the name.** `npx` resolved
  through `PATH` is a different file on two machines, and [SERVERS-10](#SERVERS-10) makes `PATH` a
  thing the declaration names. Digesting the resolved path would make an approval machine-specific,
  which is either the correct reading of what was approved or a re-approval every time a toolchain
  moves.

## Known costs

- **An approval is a channel, not code.** [SERVERS-6](#SERVERS-6) says so at the prompt and cannot do
  better. For the distribution form nearly every server uses, the digest binds the argv and the argv
  names a package whose contents change. The only real fix is pinning with an integrity hash, which
  the package ecosystems support and this declaration format does not yet express.
- **A person who approves a server has approved a program.** Beyond the confinement
  [MCP-3](mcp.md#MCP-3) requires and the capability [SERVERS-9](#SERVERS-9) grants, there is no audit
  of what it does once it runs. A confined process with a network capability is still a program
  reading a workspace.
- **Two prompts is two prompts.** A server gate and a call gate are the shape asked for, and they are
  more asking than the tool this is measured against does in aggregate, because the call gate here has
  no per-server "never ask again". The standing answers are what pay it back, and a person who
  answers 2 twice has the same quiet session with a record of having been asked.
- **The useful case costs two steps.** A newcomer to a checkout that requests a server has to declare
  it themselves and then approve it. That is the price of a shared file that cannot hand anything
  over, and it is paid by the person who wanted the server.
- **A marked description is still in the context.** [SERVERS-8](#SERVERS-8) keeps a server's words out
  of the instructions and inside a margin, and a planner that reads them has still read text somebody
  else wrote. The margin is a boundary for a person looking at a screen; for a model it is a
  convention, and conventions are what injection attacks are made of.
- **The standing answers do not travel.** Recording them outside the checkout means a second machine
  asks again and a wiped workspace does not forget. `mcp forget` is the deliberate road; there is no
  accidental one.
