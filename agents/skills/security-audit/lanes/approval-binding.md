## Lane: approval binding

A person answers a prompt about one thing and the answer is kept under a key. Later the key is looked
up for another effect, and if it matches, nobody is asked. This lane asks whether a key can match an
effect the person was never shown, now or after something about the session or the tree changes.

The bytes here are usually not untrusted. A second checkout's `.env` is a vouched file, and a binary
the planner names is the person's own. What reaches the place is the planner's request, which the
kept answer satisfies, and the gain is an effect nobody approved: the case the output contract rates
`high`.

The register is the table under `PROMPT-11` in `docs/specs/prompting.md`. It has one row per place
an answer is kept, and says what the prompt showed, what the answer is kept under, including what
that leaves out of what was shown, and how long it lasts. The spec's `kept_in` list names every
question the agent's confirmer can ask, every remembered set the terminal holds and every question
its trust prompts ask, each with the row it lands in. `make check-security` already fails where one
of those has no row, where a cell is empty, and where a named test does not exist. It reads nothing
else: a question asked another way, such as the one `/sandbox off` puts, has a row only because
somebody wrote it, so a kept answer with no row is a finding here. Nor can the check read whether the
key the code builds holds what the row says. That is this lane.

### Where to start

The register:

{register_rows}

The questions and the remembered sets, by the row each lands in:

{kept_in}

### What to ask

For each row but `one effect`, whose approvals are used once and belong to
`decisions-after-release`:

**What the lookup matches.** Find where the answer is written and where it is read back: the
function that files it and the one a later effect asks, such as `Exposed::holds` in
`crates/core/src/credentials.rs`, `TrustedPrograms::contains` in `crates/core/src/programs.rs` and
`read_exposure_is_allowed` in `crates/core/src/policy.rs`. Read what the key is built from where it
is written and what is compared where it is read. Then ask what else the comparison accepts: a
second effect this entry answers though the prompt showed something else.

**After a transition.** The same question after each thing that changes what a key's parts name:
`/cd` into a second checkout laid out like the first, `/add-dir`, `/clear`, `/branch`, `--resume`,
`/undo` and a rewind, a delegate's return, and recovery after a panic. A key relative to something
that moves names something else after the move.

**After a mutation.** The file's bytes replaced, a link on the path pointed somewhere else, the
branch switched, the same name in another directory, an argument, the environment or a redirection
changed, a server's alias or command changed, and more entries than the prompt displayed.

**Whether the row says it.** Report a row where the code's key holds less than its `Kept under`
says, or where the answer lives longer than its `Lasts`. A row that says what its key leaves out has
not thereby made that acceptable: where the clause governing the prompt says a yes covers less than
the key does, report it, and where a Known cost admits it, do not.

### The shapes this has taken

Each was fixed where it was found. They are the shapes to look for, not findings to report again.

- **The key is relative and what it is relative to moves.** A yes to reading checkout A's `.env` was
  kept as the text `.env` and answered for checkout B's after `/cd` (#955). A permission rule
  answered in one checkout was in force after `/cd` into another (#843).
- **The key is coarser than what was shown.** A standing run answer outside the root ignored the
  directory (#400). A key built from a lossy rendering of a path (#432). A key that dropped the
  redirection (#402) or ignored the environment (#326). A `<` vouch that covered any file (#146). An
  MCP grant per protocol rather than per server (#703). A guard that checked the spelling while the
  lookup used the basename (#627). `/add-dir` giving a directory a second trusted spelling (#288).
- **The answer grants more than the prompt displayed.** The rules question cut its list at the
  terminal's height while one `y` granted every rule (#954).
- **What the key names changes between the yes and its use.** A rewind writing through what a path
  resolves to now (#865).
- **A restore brings a grant back without its subject.** Undo restoring trust without the files
  (#794), a rewind point written on the capture's own trust (#706), overlapping delegate writes
  (#644), and effects left uncertain by a panic (#919).

### What is not this lane

Whether untrusted bytes reach the planner (`entry-to-planner`) or decide something after a release
(`decisions-after-release`), and whether the rows a yes rests on were drawn before it was taken
(`shown-before-answered`). A single-use endorsement replayed for a second effect belongs to
`decisions-after-release`; this lane is about answers kept for later.

A candidate names the row, what its prompt showed, the second effect the kept answer covers, and
the transition or mutation that reaches it, with the line where the lookup accepts it. An answer kept
with no row names where it is kept and the question that fills it.
