## Lane: shown before answered

Every other lane asks whether untrusted content can decide something. This one asks whether the
person who answered a prompt was shown what they answered. A prompt in the full-screen interface is
drawn in a box the terminal sizes, a card in the desktop window in a transcript the window sizes, and
much of what either shows has a length nobody at the keyboard chose: a check's sentence, a server's tool list, an argument the planner built from a file, a scan's
findings, a workspace path. A key that approves at a draw where the rows deciding the question are
below the fold collects a keypress, not a decision, and the bytes that pushed those rows off the
screen can be an attacker's.

The clause is `PROMPT-4` in `docs/specs/prompting.md`, and `VIEW-5` in
`docs/specs/terminal-transcript.md` is the small terminal. The layout is `pinned::draw` in
`crates/tui/src/confirm/pinned.rs`, which counts the deciding rows a `Seen` has had drawn, and
`Drawn::take` is where a key that approves is turned away. The table test
`no_question_takes_a_yes_before_every_row_deciding_it_has_been_drawn` draws each kind of question
with oversized content. The trust prompts in `crates/tui/src/trust_prompt.rs` have a layout of their
own. Do not report what the table already fails on.

In the desktop window the cards are the `case` arms of the transcript's entry switch in
`ui/src/renderer/components/Transcript.tsx`, and each marks the elements its answer rests on with
`data-deciding`. `useShown` and `measure` in `ui/src/renderer/shown.ts` count the marked rows that
have been on screen, and `useApproval`, `Approve` and `ApproveButton` in `Transcript.tsx` are where a
press that approves is turned away. `ui/scripts/shown.test.mjs` checks every case in
`ui/scripts/shown-cases.mjs` for a token outside the marked rows, and `ui/scripts/drive-shown.mjs`
draws each case in a small window and watches the buttons with a check of its own.

The line interface puts the same questions with a layout of its own. `Prompting` in
`crates/cli/src/plain.rs` writes the lines `bravebot_approval` builds in `crates/approval/src/lib.rs`
and then the question. The editor protocol puts each as a permission request in
`crates/ui-bridge/src/acp.rs`. A background session keeps the last part of what it wrote in
`crates/cli/src/host.rs` and replays it to a terminal that attaches later, so a question held while
nobody was attached reaches the person through that buffer. `Drawn::take` and the window's `Approve`
are on none of these paths, and the table test that draws the full-screen interface says nothing about
them.

### Where to start

The files the prompting spec governs:

{prompt_files}

Each terminal prompt, by the function that asks it, and each desktop card, by the case that draws it:

{prompt_sites}

The line interface, the editor protocol and the background host, by the function that writes a
question's lines, puts the question or replays what was written:

{other_prompt_sites}

### What to ask

For each prompt:

**What the person needs to see to decide.** The rows that say what a yes grants, what it is about,
and what a check said about it. Compare them with the rows the draw counts as deciding, or the card marks
as deciding, and with what the terminal pins above or below the part that scrolls. A row that would change the answer and is neither
counted nor pinned is a finding.

**Who controls its length.** For each of those rows, where its bytes come from: the person, the
driver, the planner, a server, a file or a check. A row whose length is somebody else's is the one
that can push the rest off the screen, and a row the driver wrote that sits after it is the one that
gets pushed.

**Whether the approving key is gated on it.** Follow each key from the event loop to the answer it
returns. Every key that approves, a standing answer and a numbered row included, has to pass through
`Drawn::take` with the `Drawn` of the draw the person last saw. A key read against a `Drawn` from
another draw, a `Seen` that outlives its request, a draw that marks rows seen that it did not draw
whole, or an approving answer built where `take` never sees it is a finding. In the window, every
button that approves has to be an `Approve` or an `ApproveButton` inside the card's `Answers`; a
button that sends an approving reply without one, a count kept across a change of width, or a row
counted while clipped, covered, faded or in a hidden window is a finding. An overlay that lets the
pointer through, as the composer's fade does, is found by no hit test, so it has to carry
`data-veil`; one drawn over the transcript without it is a finding.

**Whether the drawn text is the bytes a yes releases.** For a prompt that shows content, compare what
is drawn with what the answer then hands on. Replaced control characters are part of that. So are
characters that draw as nothing (tag characters, zero-width characters, variation selectors,
bidirectional controls): a row counted as drawn that has no glyph is a row the person did not read. A
drawn text that differs from the released bytes in a way the person cannot see is a finding whatever
the layout is.

**Whether a question survives being replayed.** A question held for a later attach, or sent again to a
client that reconnects, goes through a buffer with a cap. Ask which end the cap cuts, and whether what
the person needs (the summary, the sentence about what the model has not seen, the check's verdict)
is at the end that stays.

**Whether anything fails if it stops holding.** A kind of question with no row in the table or no case
in `shown-cases.mjs`, a row whose content fits one screen, or a row that leaves a deciding line out
of what it checks is a prompt nothing fails on. A kind the table leaves out on purpose says why beside it; read the reason
and report it only where it does not hold.

### What is not this lane

A prompt that is hard to read but whose approving key is gated on what it needs. Colour, wording and
spacing. And a finding whose whole content is that a person might not read what is on the screen,
which names the row, where its length comes from and the key that answers without it, or is not
filed. A prompt whose drawn text differs from the bytes a yes releases, or whose replay cuts the rows
that decide the question, is a different finding: name the two texts, or the cut and the rows it
removes.
