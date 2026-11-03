# Reviewing for the rule

Everything here is predicated on one statement: **untrusted content never enters the driver's
context or the planner's**. The *planner* is the model deciding what to do next. The *driver* is
this Rust code, meaning `bravebot-core` and `bravebot-agent` both. Both halves are stated as
clauses in [specs/labels.md](../specs/labels.md).

The subtle violations look like safety features, which is what makes them hard to spot. Four
things to look for in a diff:

**1. A branch on untrusted bytes.** The driver may carry untrusted content and hand it to an
effect. It may not branch on it: no `if`, `match`, comparison, or early return whose condition
derives from untrusted bytes.

```rust
// WRONG: the driver decided whether to write, from untrusted bytes.
let text = contents.declassify(&proof);
if text.matches(old).count() > 1 {
    return "error: ambiguous";
}
```

That reads like a careful refusal. It is a decision taken from bytes an attacker may have written,
and the specs refuse it: a decision may be taken only from trusted content. The right move is `Policy::read_trusted_content`, which hands over the
bytes when they are trusted and refuses otherwise, so the untrusted case cannot be taken by
accident.

**2. The same branch, moved into the kernel.** `bravebot-core` is the driver too, so relocating a
decision is not removing it. "It is only for a message to the model" does not help either, because
a message to the model *is* the planner's context.

**3. A `declassify` outside the gates.** A witness is not permission to inspect. Minting one
records that bytes moved somewhere they were already allowed to go: a filesystem write, an HTTP
body, or a person's screen. Each of those has a gate of its own, `Policy::present`,
`Policy::render_in_place` and `Policy::read_trusted_content`, and the planner's own arguments
have `Policy::read_planner_argument`. A processor's answer that a crate outside the driver reads for
one number has `Policy::read_a_number`. A reshape that reads two pieces of content at once, which
a diff is, goes through `Policy::render_pair_in_place`: the same gate, taking the taint of both
rather than one side's label. A `declassify` anywhere else is almost certainly a
violation, and it can only be written inside the policy layer: `Declassification::authorise` is
`pub(in crate::policy)`, so no other module and no other crate can mint one at all. Every file
that declassifies is pinned with a count in [specs/labels.md](../specs/labels.md), so
`make check-spec` fails on a new one until somebody records it there, which is where a reviewer
is asked whether it belongs. The gates themselves are pinned the same way, because a reshape
releases content to a closure the driver wrote and hands back a value that is still labelled, so
a new call site needs no `declassify` of its own and would otherwise move nothing. So is
`note_for` in `crates/agent/src/tools.rs`, the one function that takes a closure from its caller
and forwards it to a gate: a new caller of it writes such a closure without touching the gate.

**4. A `Labelled` built by hand.** Never construct one to give a value a better label than its
inputs had. That is laundering, whichever crate it happens in. If a value derived from
untrusted input has to be trusted for something to work, the design is wrong, not the label.
No check can tell this shape from the rest: the constructor is how the program labels its own
data, so it is used everywhere legitimately, and only a reader can say which is which. Where
[specs/labels.md](../specs/labels.md) pins a constructor with a count per file, a new site moves
that count and `make check-spec` fails until somebody records it, which puts the question in front
of a reviewer without answering it.

Ten places do branch on untrusted bytes, deliberately. Three are in the kernel, the fourth is
the search in `bravebot-agent`, the fifth is the wording of the `lsp` diagnostics sentence from
the counts a server computed, in `crates/agent/src/lsp.rs`, the sixth is `read_git`, also in
`bravebot-agent`, the seventh is the text-or-binary sniff and decoding of a read of an unvouched
file, in `Workspace::survey`, `Workspace::read` and `Workspace::page`, the eighth is the
choice of which parts of a tool result are taken, in `ToolResult::output` and `ContentBlock::picture`
in `crates/mcp/src/protocol.rs`, the ninth is the read of a processor's answer for one number,
in `bravebot_choice::number_in` in `crates/choice/src/number.rs`, and the tenth is the trigger
listener's reading of a request's framing, path, signature and body length, in
`crates/cli/src/trigger.rs`. All ten are named under
Known costs in [specs/labels.md](../specs/labels.md), because an unlisted exception is
indistinguishable from a violation. A further one that mints a witness of its own moves a pinned count and so cannot arrive
quietly. One added inside a function that already holds a counted `declassify` moves nothing: the
counts pin how many times the bytes are released, not how many decisions are then taken from them,
so `crates/core/src/policy.rs` still has to be read rather than counted. `crates/core/src/vetting.rs`
holds no `declassify` and so no count either, and the third exception reads its word there, out of
bytes `policy.rs` released. The fourth holds none either: `Workspace::grep_shaped` in
`crates/agent/src/workspace.rs` opens files with `std::fs::read_to_string` and compares each line
with `Regex::matches`, and no pinned symbol counts that, so a second comparison of the same kind
elsewhere is found by reading the diff. What the comparison decides reaches the planner through
the `search` function in `crates/agent/src/tools.rs`, whose releases are counted gates. The sixth
is the same shape in `crates/agent/src/git/`: `Repository::search_blob` and `Repository::patch`
compare a blob's bytes, and `Workspace::read_git` labels the answer from the paths shown, so a
comparison added to a third file there is found by reading the diff as well.
The seventh is the same shape: `bravebot_filetype::looks_binary` and `decode_text` hold no
`declassify`, so a second sniff or decoding of untrusted bytes elsewhere is found by reading the
diff. The eighth is the same shape in `bravebot-mcp`, which is not the driver: it holds no
`declassify`, the `Labelled::new` sites it labels with are counted, and a second comparison of a
server's `type` or `mimeType` elsewhere in the crate is found by reading the diff. What the driver
does with the sorted result is to mint a reference for each part and tell the planner how many
parts were left out, so a new branch in `bravebot-agent` on a picture's bytes, or on anything but
the count and the type from the list, is a violation of shape 1.
The crate also sorts how a call ended from what the server wrote. `isError`, an error reply, a
reply it cannot use and a line past 16 MiB are replies, and a connection closed with no reply and
a bound that passed are not. Whether a plan run goes on is decided by one thing in that sorting,
which is whether a reply arrived, so a branch in a plan run on which kind of reply arrived is a
violation of shape 1.
The ninth is the same shape in `bravebot-choice`, which is not the driver: it holds no
`declassify` and no `Labelled::new`, and the text reaches it only through `Policy::read_a_number`,
whose uses are pinned and which gives back one byte labelled as the answer was. A second comparison
of an answer's text anywhere else in that crate is found by reading the diff. What the driver does
with the byte is carry it: as a `Labelled<u8>` in the slot store, as an opaque `Carried` into
`bravebot_mcp::protocol::with_choice`, and as digits for the person's screen from
`Policy::choice_for_display`. A comparison of the number, a branch on it, a different sentence to
the planner for any value of it, or a reader whose failure depends on what the answer said is a
violation of shape 1. A second function that releases an answer's text to another crate's function
is one of shape 3 and moves a pinned count. A `Carried` turned into a number in `bravebot-core` or
`bravebot-agent` is one of shape 3 as well, and moves the count of `Carried::into_wire`.
The tenth is the trigger listener in `crates/cli/src/trigger.rs`, which is not the driver and holds
no `declassify`. It decides from the request line and headers only: the framing refusals of
`parse_head` (method, version, `Transfer-Encoding`, `Content-Length`, a path that is not a name),
the `Content-Length` against the cap, the path as a name among the person's own entries, and the
signature against the person's secret. The body is carried to the run's standard input and read by
nothing, so a branch on any byte of the body, or a header or path suffix that selects a trigger, an
event, a flag, a directory or a limit, is a violation of shape 1.

A count that comes from file-type metadata is not one of the ten. A listing or a search tells the
planner how many symbolic links its walk did not enter. The walk reads each directory entry's file
type, which is not file content, and counts the links. The planner is given a number with no name
and nothing to branch on, and nothing in the driver branches on it: the count is released beside
the other shape facts of the result (the cap, the unreadable-directory flag) through the existing
`declassify` in `list_files` and `search`, which adds no use of `declassify` to the source. A change that put a link's
name or target in that sentence, or that read the target to decide what to say, would be a violation
of shape 1 or 3 and is not covered by this paragraph.

A measurement of file names, lengths and modification times that the driver compares and reduces to
one bool is not a branch on untrusted content, for the same reason an exit status is not. The work
record's write signal for a `run` line (`crates/agent/src/workspace/tree_mark.rs`) is one: the
driver opens no file, reads no content or name into a decision (the only name match is against a
fixed list of skipped directories), and the result is one fact. A program can make that fact true
or false by what it writes, as it can with its exit status. A reviewer who sees a new measurement of
this kind checks the same three things: no file is opened, no name or content appears in a
condition, and one bounded fact comes out. A measurement that fails any of them is a violation of
shape 1 or 3 and is not covered by this paragraph.

`load_skill` also names the directory a project skill's link lands on inside the workspace. That
path is file-name content, so the rules and the trust map are asked about it first, and only a path
they let the planner read is examined further: it is dropped when it equals the name the skill is
found under or holds a control character, the same filter `files_beside` applies to each file name
before it is offered. Nothing is compared or searched in a path they refuse.

## The inverse mistake: inventing a violation

This rule is about content that could reach the planner or steer a turn in progress. It is not a
general prohibition on reading bytes that arrived over a network, and it says nothing about this
program's own configuration and startup.

`GET /v1/models` is the example to keep in mind. It is fetched before any session, from the endpoint
the user configured, so a person can pick a model off a list they read. It carries
`Label::untrusted_public()` because that label records *where bytes came from*, not that the data is
injection-sensitive, and the code already branches on it freely: `usable` filters on
`capabilities`, compares `access` against `premium`, and `adopt_window` takes a number out of the
same response to decide when a conversation is shortened. None of that is a violation, and an
argument implying it is has misread the rule.

So before reaching for a trust or label argument, ask whether the bytes could reach a model's
context or influence a turn already running. If they could not, the question is an ordinary
engineering one: latency, offline behaviour, staleness, how many code paths. Argue it on those
terms.

**Why this is worth a section.** A wrong trust argument reads exactly like a safety feature, which
is the same thing that makes a real violation hard to spot, and it is more likely to be waved
through than a plain design mistake. It also talks you out of the better implementation for a reason
that does not exist. Getting the scope of the rule wrong in this direction is a real cost, not a
harmless excess of caution.

## Going looking, rather than waiting for a diff

Everything above is a review pass, and a review pass only sees what somebody wrote this week. The
[security-audit skill](../../agents/skills/security-audit/SKILL.md) is the same four shapes turned
on the whole tree: it enumerates every site each shape could hide in, reads them in lanes, tries to
disprove what it finds, and files one issue per finding that survives. It skips anything the tracker
already holds, open or closed, and it does not create a label.

`make check-security` is its deterministic half, and it holds the three things this document
cannot. The count above has to agree with the one in [specs/labels.md](../specs/labels.md), because
two documents disagreeing about how many exceptions are admitted is how an unlisted exception
becomes invisible. Shape 3's gates have to be counted and not merely named, and so does any function
that hands a caller's own closure to one, since neither moves a count that already exists. And shape
4 says only a reader can tell a laundered label from a sound one, which is
true of a single site and not of the surface: a spec can pin how many places construct a `Labelled`
just as it pins how many release one, and a new site then arrives red rather than unread.
