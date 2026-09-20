---
id: LAYER
title: Layering
status: normative
governs:
  - crates/*/Cargo.toml
  - crates/*/src/lib.rs
  - crates/*/src/main.rs
  - crates/*/build.rs
---

## Scope

Which crate is allowed to do what. A change to a crate's dependencies or its reach is a change to
this spec.

## Clauses

<a id="LAYER-1"></a>
### LAYER-1: which crate may do what

| Crate | Purpose | Depends on | Constraint |
|---|---|---|---|
| `bravebot-core` | The information-flow kernel: the label lattice, slots, references, capabilities, and every policy gate | none | No I/O, and nothing prints. Owns every decision derived from content, and the only place a declassification witness can be minted |
| `bravebot-agent` | Task execution: the tools and the turn loop | `core`, `aichat`, `bedrock`, `config`, `i18n`, `lsp`, `net`, `sandbox`, `skus` | Carries labelled values and must not inspect them. `exec` stays argv-only, and `shell` runs only a line a person typed |
| `bravebot-net` | The network egress path for everything carrying labelled content | `core` | All agent traffic passes the policy gate here. See the known cost below |
| `bravebot-aichat` | Client for the OpenAI-compatible aichat backend | `core`, `config`, `net`, `signing` | Speaks the wire protocol only, and reaches the network through `net` |
| `bravebot-bedrock` | Client for models on AWS Bedrock | `core`, `aichat`, `config`, `net`, `signing` | Speaks the wire protocol only, and reaches the network through `net`. Runs the AWS CLI to resolve a credential, which is the one subprocess it starts |
| `bravebot-session` | What a session leaves on disk: the record, the state directory, and the audit serialiser | `core`, `agent`, `aichat`, `config`, `i18n` | Not presentation: draws nothing and links no terminal library, so a record can be read back without linking a front end. Serialises what a session held, the rules it recorded included, and decides none of it. Which build wrote a record is supplied when a session opens, since the stamp is computed where the front ends can see it and they depend on this crate rather than the other way round |
| `bravebot-tui` | The interactive terminal interface | `core`, `agent`, `aichat`, `config`, `i18n`, `net`, `sandbox`, `session` | Presentation. May display released content, always inside a margin it draws itself. Owns the clipboard and shell mode, both of which are gestures a person made. Owns the terminal itself, so it may read the tty directly to ask the terminal about itself; what comes back describes the terminal and never enters a turn |
| `bravebot-cli` | Command-line entry point | `core`, `agent`, `config`, `i18n`, `net`, `sandbox`, `session`, `skus`, `tui` | Presentation. Where nobody can be asked, effects are refused rather than applied unseen |
| `bravebot-mcp` | Model Context Protocol client: the extension boundary for tools | `core`, `net`, `sandbox` | An opaque call erases the routing/content split, so primitives stay native rather than moving behind it |
| `bravebot-lsp` | Language server client: a read-only question about a symbol | `core` | Speaks the protocol only. Asks a closed set of read-only methods and never a name a caller supplies, so a server's own method list cannot widen what this does. Separates a location from the text at it: the type carrying a location holds no text, which is what [tools/lsp.md](tools/lsp.md) rests on |
| `bravebot-sandbox` | OS-level confinement for subprocesses | none | Confines processes running code we did not write. A processor's caller is our own code, so it is not what this confines |
| `bravebot-config` | Environment-derived configuration for the backend | none | The configuration surface, on the same footing as the endpoint and the model. Reads the settings layers, including one a checkout may carry, and hands on rule text without matching a rule. See [backends.md](backends.md) |
| `bravebot-i18n` | Message catalogs for everything a person reads | none | Presentation text only. Holds nothing the planner is sent, and decides nothing: a message is named in the source, so no value can pick one. See [localization.md](localization.md) |
| `bravebot-signing` | Brave services request signing, hs2019 HMAC-SHA256 over the body digest | none | Auth only. Carries no workspace content |
| `bravebot-skus` | Imports a Leo Premium subscription by registering as a new device | none | Auth only. Carries no workspace content and no model output. Keeps its own HTTP client, over a transport its caller states. See [premium-credentials.md](premium-credentials.md) |

`verified-by: bravebot_cli::layering::every_workspace_member_is_a_row_in_the_layering_table`
`verified-by: bravebot_cli::layering::a_rows_dependency_list_is_what_the_manifest_asks_for`

<a id="LAYER-2"></a>
### LAYER-2: `bravebot-core` and `bravebot-agent` are both the driver

Relocating a decision from one into the other does not remove it. A branch on untrusted bytes is
a violation wherever it sits.

**Why.** The dependency graph makes `core` look like the safe place to put things, and it is not.
The kernel is where decisions derived from content are *taken*, not where they become allowed.

`verified-by: by-construction (a labelled value exposes no accessor for its contents, so reading one takes either a witness the policy layer alone mints or a conversion that refuses anything not already trusted and public, and labels.md pins that witness's use file by file across both crates, so a decision moved into the kernel moves a pinned count rather than escaping one; the one thing a labelled value answers without a witness is how many bytes it holds, which is a number labels.md already shows the planner)`

<a id="LAYER-3"></a>
### LAYER-3: presentation crates display untrusted content on purpose

`bravebot-tui` and `bravebot-cli` show quarantined content to the person watching. A terminal is
not a context, and an agent that will not say which file it is working on has protected nobody.
Everything shown is marked with a margin the renderer draws, and has its control characters
replaced, so the content cannot draw its own.

`verified-by: bravebot_tui::marking::quarantined_content_cannot_paint_its_own_margin`
`verified-by: bravebot_tui::render::quarantined_content_is_shown_and_marked_on_every_line`
`verified-by: bravebot_cli::progress::quarantined_content_is_shown_and_marked_on_every_line`
`verified-by: bravebot_cli::progress::quarantined_content_cannot_paint_its_own_margin`
`verified-by: bravebot_cli::layering::every_presentation_crate_is_named_by_the_clause_that_marks_content`

<a id="LAYER-4"></a>
### LAYER-4: a crate root says what it does about unsafe

Every crate root declares `#![forbid(unsafe_code)]`, or `#![deny(unsafe_code)]` with an
`#[allow(unsafe_code)]` at each site that needs one. A library, a binary and a build script are
each a crate root, and a `cfg(test)` module is part of the crate it sits in, so an `unsafe` block
there is one of the crate's own and is named the same way.

**Why.** Nearly every crate here contains no `unsafe` at all. Undeclared, that is a property
nothing records: it holds by accident, and the first `unsafe` to arrive arrives silently. Declared,
the compiler decides it, and what a reviewer reads is the sites that name themselves rather than
every crate in the workspace. Three crates name sites: `bravebot-sandbox`, whose landlock syscalls
are its reason for existing, `bravebot-skus`, whose tests point `HOME` at a scratch directory, and
`bravebot-agent`, which asks Windows for its own version ([INSTR-9](instructions.md#INSTR-9)) in
the one call a platform states that in. Taking `deny` where `forbid` would do is the way the rule
is kept in letter and lost in substance, because `deny` is the one an `allow` added later reopens.

`verified-by: bravebot_cli::unsafe_code::every_crate_root_says_what_it_does_about_unsafe`
`verified-by: bravebot_cli::unsafe_code::a_crate_that_exempts_nothing_forbids_rather_than_denies`
`verified-by: bravebot_cli::unsafe_code::allowing_unsafe_at_a_root_is_not_a_declaration`

<a id="LAYER-5"></a>
### LAYER-5: the marking rule is addressed to a surface, not to a crate

Anything that puts released content in front of a person marks it, whether it is a crate in this
workspace or a program elsewhere that links these crates. A surface can only mark content it can
still tell apart, so whatever carries content to one carries the label with it: a boundary that
drops the label has not lost a detail, it has made the content trusted by moving it.

**Why.** The crates here offer no compatibility promise, so a program outside this workspace that
links them links internal APIs across a pin of its own choosing, and a pin is not a contract. The
break it defers is found by whoever next moves it rather than by the change that caused it, and the
same distance applies to the marking: a rule written about the crates in this repository holds for
the terminal and says nothing about the screen most people are looking at. Written about surfaces,
it is a rule a second front end can be held to, which is the most this document can do about one it
does not compile.

`verified-by: bravebot_cli::layering::every_presentation_crate_is_named_by_the_clause_that_marks_content`
`verified-by: by-construction (a surface this workspace compiles is one of its crates, and the test above holds the clause naming them to every row of the table whose constraint opens on presentation, in both directions; a surface this workspace does not compile has no run to check, which is the known cost below)`

## Open questions

- **How the reach of the clause above is closed for a surface this workspace does not compile.** A
  surface built as a member lands inside the paths this spec governs, so it has to gain a row and a
  test before it compiles; a published interface with a specified transport supplies instead the
  compatibility promise the pin does not, and keeps the two release cadences apart. Both answer the
  clause and they differ in everything else, and what decides between them is who maintains what
  rather than anything here.
- **Whether the record should hold the types the front end holds in memory.** `bravebot-session`
  declares its own structs for what it writes, which is what lets a struct on disk outlive the shape
  of a struct in memory, and several of them are the interface's own live state as well: the
  questions asked beside a session, the turns a rewind can go back to and the snapshot each of those
  carries, the turns a resume replays, and a prompt in the recalled history. Those are the record's
  types being used as the interface's, which is the coupling a second front end would find rather
  than the serialiser being asked to hold the terminal's idea of a turn. Splitting them puts a second
  definition of each under the front end with a conversion between the two; leaving them is a shape
  the next front end inherits.

## Known costs

- **A crate root says nothing about the test binaries beside it.** A file under `tests/` is its
  own crate that no root attribute reaches, so the `unsafe` in `bravebot-agent`'s and
  `bravebot-tui`'s test helpers sits outside what LAYER-4 decides. Nothing in such a file ships,
  and covering them would take an attribute per file with nothing to keep a new file honest, which
  is the accident LAYER-4 exists to remove.

- **`bravebot-net` is not the only crate that opens a socket.** `bravebot-skus` builds its own HTTP
  client and talks to Brave's subscription service directly, without putting anything to the policy
  gate. That traffic carries credentials and an order id, never workspace
  content or model output, so no labelled value escapes the gate. LAYER-3 is worded as "all agent
  traffic" for that reason. A second egress that ever carried content would be a violation. What
  that client trusts and what it goes through is not its own: `register` is handed a transport
  configuration by the caller, which is `bravebot-agent` or `bravebot-cli` and depends on
  `bravebot-net` already. That is the one thing about the two clients that must not differ
  ([NET-7](network-egress.md#NET-7), [NET-8](network-egress.md#NET-8)), since a machine states one
  certificate authority and one route off it, not one per client. Passing it in rather than
  depending on `bravebot-net` keeps this crate at no dependencies, which is what makes "auth only"
  checkable by reading its manifest.

- **Nothing here reaches a surface this workspace does not compile.** The clause about marking is
  addressed to any surface, and the only surfaces checked against it are the crates in this
  workspace: the table has their rows and the clause names them. A front end elsewhere that links
  these crates is governed by nothing written down, and whether it marks what it displays is not a
  thing this repository can state either way. Markup is where that costs most, because its escapes
  are ones a terminal does not have: content that reaches raw markup, a link or a remote resource
  can draw its own container and can leave the machine, so a margin is the first of three questions
  rather than the whole of one.

- **Reading a session record still links the agent.** `bravebot-session` links no terminal library,
  which is what a second front end wanted from the move, and it is not a leaf: a record holds what a
  turn produced, so the conversation, the backups a rewind can restore and the timing are
  `bravebot-agent`'s types, and `bravebot-agent` reaches `bravebot-net` and `bravebot-sandbox`. A
  program that only wants to list what sessions exist therefore links the turn loop. Declaring the
  record's own version of each of those types would cut the edge and put a second definition of
  every one of them under this crate, which is a copy to keep in step for a caller nothing has yet
  asked for.
