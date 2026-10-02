## Lane: shown before answered

Every other lane asks whether untrusted content can decide something. This one asks whether the
person who answered a prompt was shown what they answered. A prompt in the full-screen interface is
drawn in a box the terminal sizes, and much of what it shows has a length nobody at the keyboard
chose: a check's sentence, a server's tool list, an argument the planner built from a file, a scan's
findings, a workspace path. A key that approves at a draw where the rows deciding the question are
below the fold collects a keypress, not a decision, and the bytes that pushed those rows off the
screen can be an attacker's.

The clause is `PROMPT-4` in `docs/specs/prompting.md`, and `VIEW-5` in
`docs/specs/terminal-transcript.md` is the small terminal. The layout is `pinned::draw` in
`crates/tui/src/confirm.rs`, which counts the deciding rows a `Seen` has had drawn, and
`Drawn::take` is where a key that approves is turned away. The table test
`no_question_takes_a_yes_before_every_row_deciding_it_has_been_drawn` draws each kind of question
with oversized content. The trust prompts in `crates/tui/src/trust_prompt.rs` have a layout of their
own. Do not report what the table already fails on.

### Where to start

The files the prompting spec governs:

{prompt_files}

Each prompt, by the function that asks it:

{prompt_sites}

### What to ask

For each prompt:

**What the person needs to see to decide.** The rows that say what a yes grants, what it is about,
and what a check said about it. Compare them with the rows the draw counts as deciding and with what
it pins above or below the part that scrolls. A row that would change the answer and is neither
counted nor pinned is a finding.

**Who controls its length.** For each of those rows, where its bytes come from: the person, the
driver, the planner, a server, a file or a check. A row whose length is somebody else's is the one
that can push the rest off the screen, and a row the driver wrote that sits after it is the one that
gets pushed.

**Whether the approving key is gated on it.** Follow each key from the event loop to the answer it
returns. Every key that approves, a standing answer and a numbered row included, has to pass through
`Drawn::take` with the `Drawn` of the draw the person last saw. A key read against a `Drawn` from
another draw, a `Seen` that outlives its request, a draw that marks rows seen that it did not draw
whole, or an approving answer built where `take` never sees it is a finding.

**Whether anything fails if it stops holding.** A kind of question with no row in the table, a row
whose content fits one screen, or a row that leaves a deciding line out of what it checks is a
prompt nothing fails on. A kind the table leaves out on purpose says why beside it; read the reason
and report it only where it does not hold.

### What is not this lane

The line interface and the desktop app, which put the same questions in other front ends with
layouts of their own. A prompt that is hard to read but whose approving key is gated on what it
needs. Colour, wording and spacing. And any finding whose whole content is that a person might not
read what is on the screen: name the row, where its length comes from, and the key that answers
without it, or do not file it.
