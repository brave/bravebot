## Lane: untrusted text reaching a terminal, a window or a file another program reads

Other lanes ask whether untrusted content can decide something. This one asks what an untrusted
string does to the surface it is written to. A terminal treats an escape sequence as an instruction,
a window treats markup as a page, and a program that reads a file treats its content as input. Text a
person did not write reaches each of them from a clone, a server, a program's output or the planner.
A string drawn with its control characters, or with characters that draw as nothing, can hide what the
person is deciding on, draw a different text from the bytes a yes releases, or change the terminal's
state for everything after it.

The clauses are `LAYER-3` in `docs/specs/layering.md`, which has the presentation crates show
quarantined content with its control characters replaced, `VIEW-3` and `VIEW-4` in
`docs/specs/terminal-transcript.md`, `PROMPT-1` in `docs/specs/prompting.md` and `OUTPUT-1` in
`docs/specs/tools/read-output.md`. The functions that make text safe to draw are `printable` in
`crates/tui/src/render.rs` and in `crates/approval/src/lib.rs`, which replace control characters,
`draws_as_nothing` and `spelled` in `crates/core/src/invisible.rs`, which list and write out the
characters that draw as nothing, `shown_name` in `crates/agent/src/confirm.rs` and `pictured` in
`crates/ui-bridge/src/acp.rs`. Read what each one replaces before you judge a site. A site that
replaces `char::is_control` alone leaves tag characters, zero-width characters, variation selectors
and bidirectional controls in the text, and those draw as nothing.

### Where to start

{output_count} writes in `crates/cli`, `crates/tui` and `crates/approval` whose arguments are not all
literals: `println!`, `eprintln!`, `print!`, `eprint!`, `writeln!`, `write!`, `t!` and `say`. By file,
with the functions to open. `surface.json` lists every line and the kinds found on it:

{output_files}

Where the desktop window's source puts a string into the page as preformatted text or markup:

{window_sinks}

`crates/cli/src/host.rs` keeps the last part of what a background session wrote and replays it to a
terminal that attaches later. It is a write to a surface too, and the list above does not reach it.

### What to ask

For each file, take the writes whose arguments come from somewhere other than the program.

**Where the text comes from.** The person, the program, the planner, a server, a file in the clone, a
settings layer, a program's output, or a path or name on disk. The last five are somebody else's. A
string the program built from constants is out of scope.

**What it passes through on the way.** A function that replaces control characters, one that also
removes invisible characters, or neither. A string drawn through `{{}}` with no call between the
source and the macro is drawn as it came. Compare sibling sites: a function applied to a name and left
off the text beside it is a finding about the text.

**Where the text is drawn.** A terminal line, a prompt a person answers, a replay of a held question,
an editor protocol message, a log, a file another program reads. A prompt carries the highest stakes
because the text there is what a yes releases, so ask whether the drawn text is the bytes.

**Whether it arrives whole.** A buffer, a cap or a cut between the write and the draw. Ask which end a
cut removes, and whether the rows the person needs to decide (a summary, a sentence about what the
model has not seen, a check's verdict) are at the end that stays.

**When it is drawn.** Before the trust question, before the session draws its own state, or after a
hide or conceal sequence that nothing resets.

**What the window does with it.** In `ui/`, `<pre>`, `innerHTML` and `dangerouslySetInnerHTML` each put
a string somewhere different. Check whether the string is escaped, whether it is markup from the
program or from outside, and whether the card marks it as deciding.

### What is not this lane

Text the program wrote from constants. Text already drawn through a function that removes control and
invisible characters, where the only complaint is wording or layout. A prompt that is hard to read
where the approving key is gated on the deciding rows having been drawn, which is the
shown-before-answered lane's. And a finding whose whole content is that a terminal could misdraw
something: name the string, where its bytes come from, the writer that leaves them in and what the
person is deciding when it is drawn, or do not file it.
