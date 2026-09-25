---
id: LAYER
title: Layering
status: normative
governs:
  - crates/*/Cargo.toml
  - crates/*/src/lib.rs
  - crates/*/src/main.rs
  - crates/*/build.rs
  - crates/*/examples/*.rs
  - crates/agent/src/conversation.rs
  - crates/ui-bridge/src/wire.rs
  - crates/ui-bridge/src/fork.rs
  - ui/src/renderer/components/Transcript.tsx
  - ui/src/renderer/components/Markdown.tsx
  - ui/src/renderer/transcript.ts
  - ui/scripts/marking.test.mjs
  - ui/scripts/ux-state.test.mjs
documented-by: none (internal: which crate may depend on what is how the repository is built, not something a reader acts on)
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
| `bravebot-agent` | Task execution: the tools, the turn loop, and the standing watches a session keeps | `core`, `aichat`, `bedrock`, `config`, `i18n`, `lsp`, `net`, `sandbox`, `skus` | Carries labelled values and must not inspect them. `exec` stays argv-only, and `shell` runs only a line a person typed |
| `bravebot-net` | The network egress path for everything carrying labelled content | `core` | All agent traffic passes the policy gate here. See the known cost below |
| `bravebot-aichat` | Client for the OpenAI-compatible aichat backend | `core`, `config`, `net`, `signing` | Speaks the wire protocol only, and reaches the network through `net` |
| `bravebot-bedrock` | Client for models on AWS Bedrock | `core`, `aichat`, `config`, `net`, `signing` | Speaks the wire protocol only, and reaches the network through `net`. Runs the AWS CLI to resolve a credential, which is the one subprocess it starts |
| `bravebot-session` | What a session leaves on disk: the record, the state directory, and the audit serialiser | `core`, `agent`, `aichat`, `config`, `i18n` | Not presentation: draws nothing and links no terminal library, so a record can be read back without linking a front end. Serialises what a session held, the rules it recorded included, and decides none of it. Which build wrote a record is supplied when a session opens rather than read here, so this crate does not depend on the crate holding the stamp: a record states the build its caller was, which is the build that wrote it, and a resume reads that back rather than overwriting it with its own |
| `bravebot-tui` | The interactive terminal interface | `core`, `agent`, `aichat`, `config`, `i18n`, `net`, `sandbox`, `session`, `stamp` | Presentation. May display released content, always inside a margin it draws itself. Owns the clipboard and shell mode, both of which are gestures a person made. Owns the terminal itself, so it may read the tty directly to ask the terminal about itself; what comes back describes the terminal and never enters a turn |
| `bravebot-cli` | Command-line entry point | `core`, `agent`, `config`, `i18n`, `net`, `sandbox`, `session`, `skus`, `stamp`, `tui` | Presentation. Where nobody can be asked, effects are refused rather than applied unseen. Writes the one file that declares an MCP server, only from a command a person typed, and records an approval of one only on their answer at a terminal. See [mcp-servers.md](mcp-servers.md) |
| `bravebot-mcp` | Model Context Protocol client: the extension boundary for tools | `core`, `net`, `sandbox` | An opaque call erases the routing/content split, so primitives stay native rather than moving behind it |
| `bravebot-lsp` | Language server client: a read-only question about a symbol | `core` | Speaks the protocol only. Asks a closed set of read-only methods and never a name a caller supplies, so a server's own method list cannot widen what this does. Separates a location from the text at it: the type carrying a location holds no text, which is what [tools/lsp.md](tools/lsp.md) rests on |
| `bravebot-sandbox` | OS-level confinement: what a subprocess may reach, and what a crash of this process may leave behind | none | Confines processes running code we did not write. A processor's caller is our own code, so it is not what this confines. Also holds what a process says to the kernel about its own memory image, which is a platform call rather than a policy and belongs with the other ones: the crate that owns a credential depends on nothing and forbids `unsafe`, so it can clear a buffer and cannot tell the kernel anything |
| `bravebot-config` | Environment-derived configuration for the backend | none | The configuration surface, on the same footing as the endpoint and the model. Reads the settings layers, including one a checkout may carry, and hands on rule text without matching a rule. Reads an MCP server's declaration from the person's own directory and from no layer, and records a layer that tries to make one rather than acting on it. See [backends.md](backends.md) and [mcp-servers.md](mcp-servers.md) |
| `bravebot-i18n` | Message catalogs for everything a person reads | none | Presentation text only. Holds nothing the planner is sent, and decides nothing: a message is named in the source, so no value can pick one. See [localization.md](localization.md) |
| `bravebot-signing` | Brave services request signing, hs2019 HMAC-SHA256 over the body digest | none | Auth only. Carries no workspace content |
| `bravebot-stamp` | Which build this is: the version, the commit it was built from, and whether that tree was modified | none | Not presentation: draws nothing and links no terminal library, so a front end that draws nothing can still name the build it is. Asks git at compile time and holds one string; carries no workspace content and decides nothing |
| `bravebot-skus` | Imports a Leo Premium subscription by registering as a new device | none | Auth only. Carries no workspace content and no model output. Keeps its own HTTP client, over a transport its caller states. See [premium-credentials.md](premium-credentials.md) |
| `bravebot-ui-bridge` | Drives a turn for the graphical front end, over newline-delimited JSON on a pipe | `core`, `agent`, `aichat`, `config`, `net`, `session`, `stamp` | Not presentation: draws nothing and links no terminal library, because the surface it serves is a renderer in another process. Serialises labelled values across the transport without inspecting them, and carries the label with the content rather than dropping it at the boundary. Composes no request to a service and decodes no reply from one: what a service serves is asked of the crate that speaks to it, so a field two front ends want differently widens that crate rather than being assembled here. Only the transport binary writes to stdout or ends the process, since a stray print elsewhere would interleave with the protocol |
| `bravebot-ui-files` | Reads and writes the files the graphical front end is asked to open, under a directory it is handed | none | Reads and writes only under a directory its caller pins, resolving each path component relative to it and never following a link, so a path cannot leave the tree a person opened. Runs no command and hands nothing to a turn. The walk is `openat` on POSIX and `NtCreateFile` relative to a handle on Windows, the one native call there that opens relative to a directory. Which directories may be pinned is [trust-map.md](trust-map.md)'s question rather than this document's |

`verified-by: bravebot_cli::layering::every_workspace_member_is_a_row_in_the_layering_table`
`verified-by: bravebot_cli::layering::a_rows_dependency_list_is_what_the_manifest_asks_for`
`verified-by: bravebot_cli::layering::a_crate_that_draws_nothing_reaches_no_terminal_library`

<a id="LAYER-2"></a>
### LAYER-2: `bravebot-core` and `bravebot-agent` are both the driver

Relocating a decision from one into the other does not remove it. A branch on untrusted bytes is
a violation wherever it sits.

**Why.** The dependency graph makes `core` look like the safe place to put things, and it is not.
The kernel is where decisions derived from content are *taken*, not where they become allowed.

`verified-by: by-construction (a labelled value exposes no accessor for its contents, so reading one takes either a witness the policy layer alone mints or a conversion that refuses anything not already trusted and public, and labels.md pins that witness's use file by file across both crates, so a decision moved into the kernel moves a pinned count rather than escaping one; the one thing a labelled value answers without a witness is how many bytes it holds, which is a number labels.md already shows the planner)`
`verified-by: by-construction (the one class of untrusted bytes the kernel holds outside a labelled value is the address a slot carries, which is a bare string, so every accessor returning one takes a witness minted only inside the policy module, bar the single gate that releases the sentence for the screen asking about the slot, and labels.md pins their uses file by file as well; a decision on a slot's address relocated into another module of bravebot-core either does not compile or moves that one gate's pinned count, and one relocated into the policy module moves a pinned count)`

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
`#[allow(unsafe_code)]` at each site that needs one. A library, a binary, an example, a benchmark
and a build script are each a crate root, and a `cfg(test)` module is part of the crate it sits in,
so an `unsafe` block there is one of the crate's own and is named the same way.

**Why.** Nearly every crate here contains no `unsafe` at all. Undeclared, that is a property
nothing records: it holds by accident, and the first `unsafe` to arrive arrives silently. Declared,
the compiler decides it, and what a reviewer reads is the sites that name themselves rather than
every crate in the workspace. Four crates name sites: `bravebot-sandbox`, whose landlock
syscalls and whose Win32 calls creating a process inside a container are its reason for existing,
`bravebot-skus`, which asks Windows for the access-control list that
keeps an imported subscription to one account ([PREM-7](premium-credentials.md#PREM-7)) and whose
tests point `HOME` at a scratch directory, and `bravebot-agent`, which asks Windows for its own
version ([INSTR-9](instructions.md#INSTR-9)) in the one call a platform states that in, and
`bravebot-ui-files`, whose Windows walk opens, renames and removes relative to a handle in calls
only the native API offers. Taking `deny` where `forbid` would do is the way the rule
is kept in letter and lost in substance, because `deny` is the one an `allow` added later reopens.

`verified-by: bravebot_cli::unsafe_code::every_crate_root_says_what_it_does_about_unsafe`
`verified-by: bravebot_cli::unsafe_code::the_rule_reaches_every_example_and_benchmark_beside_a_crate`
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

**The desktop renderer is such a surface, and it is in this repository.** `ui/` is a React front
end over [LABEL-10](labels.md#LABEL-10)'s transport, built by `npm` rather than by this workspace,
so no row of LAYER-1's table reaches it and no Rust test renders it. Markup gives content two
escapes a terminal does not have, so the marking rule reaches it as three properties rather than
one:

- **Released content is marked by a container the renderer draws, and cannot produce a second
  one.** A quarantined preview sits inside a block whose head says `confined` and names the origin
  and whose foot states the reach; an untrusted write is marked on the card. Content reaches the
  tree as a text child rather than as markup, so its own spelling of that chrome is drawn as the
  characters it is: neutralised rather than dropped, for the reason the terminal neutralises an
  escape rather than removing it. A question whose turn ended before anybody answered it is drawn
  as the card it was: it loses the controls, because the channel the answer would go down is gone,
  and it keeps everything that marks its content, because a turn ending declassifies nothing.
- **Content reaches no raw markup.** No plugin turning HTML in released content into elements is
  installed, and nothing in the front end hands an element `dangerouslySetInnerHTML` or assigns
  `innerHTML`. That is what makes the property above a property of the renderer rather than of
  what the content happens to hold. An absence is checked as one rather than remembered: the test
  named below reads every source under `ui/src` and the dependencies `ui/package.json` declares.
  What is forbidden is the use rather than the word: `src/main/export.ts` names the route in the
  paragraph explaining why the PDF window exists, and a rule against writing that down would cost
  more than it bought.
- **Nothing content renders makes the app fetch.** An image in released content is never an
  `<img>`, and no element the renderer draws around such content carries an attribute a browser
  resolves without being asked: no `src`, no `srcset`, no `url(` inside a style. So nothing
  leaves the machine unless the person picks it, and what they may pick is bounded as well.
  Following a link leaves the app only where a URL parser reads its scheme as `http:`, `https:` or
  `mailto:`, and a relative path is not a link out at all: it becomes a preview inside this window,
  and only where the path stays within the project. This is the question a terminal never had to
  answer, and the answer is a capability rather than a decoration: the main process answers a
  window-open by handing the URL to the operating system.

Formatting is part of the marking rather than beside it. Markdown is applied to the assistant
bubble and to nothing else, so a heading or a bold run is itself a statement that these words came
from the planner. Giving quarantined content that vocabulary would hand it the signals the reader
is meant to trust, an inch above them.

`verified-by: bravebot_cli::layering::every_presentation_crate_is_named_by_the_clause_that_marks_content`
`verified-by: by-construction (a surface this workspace compiles is one of its crates, and the test above holds the clause naming them to every row of the table whose constraint opens on presentation, in both directions; the desktop renderer is not one of its crates and is pinned instead by ui/scripts/marking.test.mjs, which renders the real components through react-dom and asserts the three properties above on the markup that comes out, for every kind of entry the transcript has and for each of those both as a live card and as one whose turn ended unanswered; the set of kinds is read out of the Entry union in transcript.ts rather than listed by hand, so a kind added without a card drawn for it fails the test, and one that shows no released content is recorded there with the reason, which is the half no grep decides; the second property is asserted as the absence it is stated as, over every source under ui/src and the dependencies ui/package.json declares; and make check-ui and the Front end CI job both run all of it, while the governs list above holds the file's existence to make check-spec; a surface in neither place has no run to check, which is the known cost below)`

<a id="LAYER-6"></a>
### LAYER-6: what a message is comes from the record, not from its words

A conversation holds messages composed rather than typed: a file somebody named, put in front of
the planner as a user-role message, a watch that fired while no turn was running to notice it, and
a prompt a front end sent on its own account rather than on anybody's instruction. Each is recorded
with a tag saying which it is, the tag rides beside the message rather than inside it so that no
part of it reaches a backend, and a surface drawing the conversation back decides what to draw from
the tag. Reading the words to decide instead is a
violation, even where those words are the agent's own. A surface whose transcript has no row of its
own for a tag draws the message plainly, which is why the record reports the words beside the tag;
what crosses to a surface in another process is the tag alone, since a client offered both is offered
the choice this removes.

**Why.** The words of an attached message are a line the agent wrote followed by the file's own
bytes. A surface recognising the message by that line hands whoever wrote the file the choice of
which row it appears as, the rows the interface draws about itself included, which reopens one level
up the escape [LAYER-5](#LAYER-5) closes for chrome. It decides a count as well: the places a
conversation may be cut are the prompts a person typed, so a surface taking a composed message for a
prompt, or a typed prompt for a composed one, numbers them differently from the agent and cuts in the
wrong place or refuses a cut that was asked for. A tag the composer writes leaves the choice with the
composer, which is the discipline [labels.md](labels.md)'s transport already follows everywhere else:
what crosses is the discriminant, never prose for a reader to parse.

The tags the agent writes are its account of what a turn did, so a front end may name the tag for
a prompt it composed itself and may name none of the agent's: one that could would be choosing the
row a line a person typed is drawn as, by another route. A request naming a tag it may not have is
refused rather than sent untagged, since an untagged prompt is drawn as one somebody typed.

A record written before the tag existed holds a bare message where this holds a message and a tag,
and reads as a message nobody composed, which is what it was. A tag a surface draws no row of its own
for, because the build does not recognise it or because that transcript has no such row, is drawn as
a plain message and never as a row the interface writes itself, since such a row asserts something
about the conversation that the build cannot check; drawn rather than dropped, since a message a
transcript leaves out silently is the failure the tags exist to prevent, and for a file read into a
turn it is the context that turn worked from.

`verified-by: bravebot_agent::turn::a_file_the_turn_admits_is_recorded_as_a_message_the_agent_composed`
`verified-by: bravebot_agent::turn::a_prompt_the_agent_composed_is_recorded_as_one`
`verified-by: bravebot_agent::conversation::a_message_the_agent_composed_is_recorded_as_one`
`verified-by: bravebot_agent::conversation::the_tag_is_not_part_of_what_the_planner_is_sent`
`verified-by: bravebot_agent::conversation::a_record_written_before_the_tag_still_reads`
`verified-by: bravebot_ui_bridge::wire::a_message_the_agent_composed_crosses_as_a_tag_and_no_prose`
`verified-by: bravebot_ui_bridge::wire::a_prompt_a_front_end_composed_crosses_as_a_tag_and_no_prose`
`verified-by: bravebot_ui_bridge::wire::a_front_end_may_name_its_own_tag_and_none_of_the_agents`
`verified-by: bravebot_tui::state::a_replayed_composed_message_is_drawn_as_the_message_it_was`
`verified-by: bravebot_ui_bridge::fork::a_file_the_agent_put_in_front_of_the_planner_is_not_a_prompt`
`verified-by: bravebot_ui_bridge::fork::a_prompt_that_reads_like_a_composed_message_is_still_where_the_cut_lands`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so it is pinned instead by ui/scripts/ux-state.test.mjs, which loads the real transcript module and asserts that a tagged message is drawn from its fields, that a typed prompt imitating one, the app's own house-keeping included, is drawn as the prompt somebody typed and keeps the ordinal a fork of it cuts on, that a tag the build does not know is quoted rather than dropped, and that the two places counting prompts count the same list; make check-ui and the Front end CI job both run it, while the governs list above holds the file's existence to make check-spec)`

## Open questions

- **How the reach of [LAYER-5](#LAYER-5) is closed for a surface in another repository.** The desktop
  renderer answers it for one in this repository and the answer does not travel: it lands inside the
  paths this spec governs, so a diff under it is read against this clause, and it carries a test
  runner of its own, so a CI job decides it. For a surface elsewhere, one built as a workspace
  member lands inside those paths too and has to gain a row and a test before it compiles; a
  published interface with a specified transport supplies instead the compatibility promise the pin
  does not, and keeps the two release cadences apart. Both answer the clause and they differ in
  everything else, and what decides between them is who maintains what rather than anything here.
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

- **A front end composes one request of its own.** The rule is that it composes none: asking a
  service what it serves belongs to the crate that speaks to that service, and a front end asks that
  crate. One listing path does it anyway, assembling the request, choosing between a narrow and a
  wide route, and decoding the answer, all of which already exist in the crate for that service. It
  was copied because the shared version returns less than a graphical picker draws, which is a reason
  to widen that function rather than to fork the road to it.

  What it costs is not the duplication. Decoding a reply off the network is a declassification, so
  [labels.md](labels.md) pins a guarded symbol to a front-end crate that would otherwise hold none,
  and the entry stands for as long as the fork does. Tracked as
  [issue #574](https://github.com/brave/bravebot/issues/574).

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

- **What the desktop renderer is held to is not in `make check`.** The properties
  [LAYER-5](#LAYER-5) and [LAYER-6](#LAYER-6) state for it are asserted by Node tests rather than
  Rust ones, so they are gated by `make check-ui`, which is in `check-all` and not in `check`. A
  renderer change that drops the marking, or that goes back to reading a message's words to decide
  what a row is, therefore passes the check a commit is expected to pass.

- **A front end in another repository reaches nothing here.** The clause about marking is addressed
  to any surface, and the surfaces checked against it are the crates in this workspace and the
  renderer beside them. A front end elsewhere that links these crates is governed by nothing
  written down, and whether it marks what it displays is not a thing this repository can state
  either way. Markup is where that costs most, because its escapes are ones a terminal does not
  have: content that reaches raw markup, a link or a remote resource can draw its own container and
  can leave the machine, so a margin is the first of three questions rather than the whole of one.

- **Reading a session record still links the agent.** `bravebot-session` links no terminal library,
  which is what a second front end wanted from the move, and it is not a leaf: a record holds what a
  turn produced, so the conversation, the backups a rewind can restore and the timing are
  `bravebot-agent`'s types, and `bravebot-agent` reaches `bravebot-net` and `bravebot-sandbox`. A
  program that only wants to list what sessions exist therefore links the turn loop. Declaring the
  record's own version of each of those types would cut the edge and put a second definition of
  every one of them under this crate, which is a copy to keep in step for a caller nothing has yet
  asked for.
