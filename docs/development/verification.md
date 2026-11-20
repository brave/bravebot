# Verification

[Verus](https://github.com/verus-lang/verus) checks that a function meets its contract for every
input. A test or a fuzz target runs some inputs and finds a counterexample if it meets one; a proof
says there is none. Contracts sit on the code that ships, so an edit that breaks one fails
`make check-verus`.

## What has a contract

| Item | Contract |
|---|---|
| `Integrity::meet`, `Confidentiality::join` | Each returns the pessimistic answer |
| `Label::degrades_to` | It is the axis-by-axis order taint travels in, which `crates/core/src/spec.rs` proves a lattice with `(T,priv)` and `(U,pub)` incomparable, in which the taint of two labels is their least upper bound |
| `Label::is_trusted`, `Label::is_public` | Each reads its axis |
| `Labelled::label`, `Labelled::declassify` | Each returns the label or the value held |
| `Labelled::into_trusted` | The value comes out exactly when the label is `(T,pub)`, and otherwise the labelled value comes back unchanged |
| `Labelled::relabel` | It succeeds exactly when the new label is a degradation, and keeps the value |
| `Policy::refuse_untrusted`, `Policy::read_trusted_content` | Each refuses exactly when the label is untrusted |
| `Labelled::new`, `Labelled::trusted`, `Label::new` | Each builds the value and label it is given, `trusted` at `(T,pub)` |
| `Policy::read_planner_argument`, `Policy::promote_confined_read` | Each passes exactly when the argument is public and the planner's context has met nothing untrusted, and `promote_confined_read` returns `(T,pub)` |
| `Policy::declassify`, `Policy::declassify_into_place` | Each releases exactly the slots the release plan named before any content was seen, `declassify_into_place` only to the destination named with the slot |
| `Policy::declassify_into_workspace`, `Policy::declassify_written_into_workspace`, `Policy::declassify_checkout_into_workspace` | Each returns the value at public with the integrity it had, so a release never raises integrity |
| `Policy::endorsed_for_reading` | `read_output`, `promote_vetted` and `promote_vetted_picture` hand a slot to the planner only where it is the kind the gate serves and a single-use endorsement for it was issued |
| `Policy::promote_a_tool_list`, `Policy::promote_a_server_move` | Each passes exactly when a single-use endorsement for the server's alias was issued, and returns `(T,pub)` |
| `Policy::adopt_edited_body` | It adopts a body a person edited exactly when the proposed body was `(T,pub)`, and records the saved body at `(T,pub)` |
| `Policy::record_answers` | It records a reply exactly when the questions were trusted, at the questions' own label |
| `Policy::label_user_command_output`, `Policy::label_user_configuration`, `Policy::label_kept_fact` | Each gives the first label its provenance names: `(T,priv)` for a command the person typed, `(T,pub)` for their configuration, and trusted at the confidentiality the fact was kept at |
| `Plan::can_be_remembered` | It allows a line exactly when everything that decides what each step does is in the entry `a` records, as `spec::plan_is_its_entries` states over every field of `Plan` and `Step`: no file written or read by name, no reference fed in, no assignment, and no route that opens a file |
| `Plan::can_be_remembered_as` | It allows a request exactly when its plan can be remembered and it asked for no scope, did not ask to run with no profile, and writes no protected path. `Plan::vouch` makes the only `Vouch`, and `Policy::remember_vouch` records only from one |
| `policy::run_reason` | Private input, a requested scope, a write, an assignment, and a line outside the root that no entry names are always put to a person, and so is a line an `ask` or `deny` rule matches. A line goes unasked only on the answer that covers it, and a fed reference only on an `allow` rule or the audited table |
| `policy::output_label` | Output is trusted only on a proof from the audited table or a vouch for every step, never for a line that asked for a scope or to run unconfined, and never on a vouch for a line carrying an assignment or fed untrusted content |
| `policy::write_reason`, `WriteReason::asks` | A write to a destination nobody has read or to a protected path is always put to a person, whatever a rule says, and so is one an `ask` or `deny` rule matches. A write goes unasked only on an `allow` rule, or on the trust map where it answers: never into a path nobody has vouched for, and never with untrusted data into a trusted path |
| `trust::meet_of_rules`, `policy::weakest_name` | A subtree or a file asked about under several names is trusted only where every rule bearing on it, and every name, answers trusted. An undecided boundary, a name nothing answers, and a file with no name count as untrusted, so a rule can only lower the answer (`spec::a_meet_of_rules_is_trusted_only_where_every_rule_is`) |
| `trust::weakest_decision`, `trust::weaker`, `trust::strength` | Where several spellings of one name hold a decision, the one that grants least decides: a distrust before an undecided boundary before a trust |
| `TrustedPrograms::covers_every_step` | Every step a plan runs, including each step inside a `$(...)`, has an entry equal to it in file, start path, arguments and tree |
| `Remembered::covers`, `RememberedStep::matches`, `Shape::matches` | A remembered line covers a plan exactly where the lines are joined the same way and each step matches in name, file, start path, assignments, routes and arguments, with a number slot admitting only a number, and the step carries no expansion |
| `permissions::ruling_of` | A matching `deny` decides over every other rule, an `ask` over an `allow`, and an `allow` only where nothing restricts |
| `permissions::pipeline_decision` | A `deny` on any stage denies a pipeline, an `ask` on any stage asks, and a pipeline is allowed only where it has stages and every one is allowed |

`verify/mutations.txt` lists a defect for each, and `make check-verus` requires Verus to refuse
every one.

`verify/history.rs` is a copy of three fixes from the history, 3facf14a, e0cd1903 and 20ce6086,
with a contract each, written before the crates they are in carried contracts. Verus refuses each
under the `before_<commit>` cfg that restores the code before the fix. A copy can drift from the
code, so a fix there moves into its crate as a contract when the crate is set up for Verus.

## When to write one

Code a reviewer would otherwise have to trust gets a contract. Write one with:

- a gate, or anything that decides from a label, so the outcome is stated as a function of the
  label and not of the content;
- a change to the label lattice or to `Labelled`;
- arithmetic, an index or a slice on a length or an offset that came from outside: a frame header,
  a reply, a file, a prompt the person edited. A wrapped sum or a slice past the end is a crash
  under `panic = "abort"` or a wrong number in a release build. 3facf14a, e0cd1903 and 20ce6086
  were each this, and each shipped.

A fix for a panic or an overflow comes with the contract that refuses the old code, and a mutation
that restores it. That is the regression test that holds for every input.

## Writing one

Only `bravebot-core` is set up for Verus today. `crates/core/src/lib.rs` imports the Verus prelude
behind `cfg(verus_keep_ghost)`, which Verus sets and nothing else does, so a build without Verus
compiles none of what follows.

A function's contract is an attribute on it:

```rust
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r == crate::spec::meet(self, other),
))]
pub fn meet(self, other: Self) -> Self {
```

- `requires` states what the caller owes, `ensures` what the function returns, and a loop takes
  `invariant` and `decreases` the same way, on the loop.
- A module that holds a contract imports `vstd::prelude::*` behind `cfg(verus_keep_ghost)`, and
  a crate with a contract on a loop enables `proc_macro_hygiene` behind the same cfg, as
  `crates/core/src/lib.rs` does.
- A type a contract reads takes `#[cfg_attr(verus_keep_ghost, verus_verify)]`, and
  `derive(Structural)` there too where a contract compares it with `==`.
- Definitions a contract names, and proofs, go in `crates/core/src/spec.rs` inside `verus! { }`.
  Write them from what the code is for, not by copying the body: a spec copied from the code proves
  the code equals itself. A proof of what a spec clause says is cited under the clause as
  `verified-by: bravebot_core::spec::<proof>`, and `make check-spec` fails if it does not exist.
- A private field is read in a contract through a `closed spec fn` written beside the type, as
  `spec_label_of` and `spec_value_of` are in `crates/core/src/value.rs`. A spec function exists only
  for Verus, so it opens nothing in a build, and it is not a function of the type's own.
- A function Verus cannot read, such as one building a `String` with `format!`, takes
  `verus_verify(external_body)`, and then its contract is assumed rather than proved. Keep such
  functions to the ones a reviewer can check by eye, and say in the pull request which they are.
  `verify/contracts.txt` lists each one, with every other place Verus is told to accept something
  without proof, so a new one shows in the diff.

Then:

1. Add a mutation to `verify/mutations.txt` that breaks what the contract states, and run
   `make check-verus`. The crate has to verify and the mutation has to be refused.
2. Run `python3 contrib/check-verus-contracts.py --write`, which records the contract, and any
   assumption the change adds, in `verify/contracts.txt`.
3. Say in the pull request what each new contract states and which mutation shows it can fail.

## What the checks hold

- `make check` and `make check-scripts` run `contrib/check-verus-contracts.py`, which needs no
  toolchain. It fails when a contract in `crates/` differs from `verify/contracts.txt`, so a
  contract that is removed, added or weakened shows in the diff. It fails the same way when an
  assumption is added or removed in a file that holds Verus code: an `external_body`, `external`,
  `external_*_specification`, `assume_termination` or `assume_compatibility_with_trait_spec`
  attribute, an `assume` or `admit`, or an `assume_specification`. Verus verifies a crate with
  any of these, so the ledger is what shows a reviewer each one. CI runs it in the Security job.
- `make check-verus` runs Verus. Each crate in `CRATES` in `contrib/check-verus.py` has to verify,
  each mutation has to be refused, and `verify/history.rs` has to verify and be refused under each
  `before_<commit>` cfg.

CI's `Verus` job runs `make check-verus` on a pull request that changes `crates/core/`, `verify/`
or the scripts that run them, on the merge queue's branch, and on `main`. It installs the release
`VERUS_VERSION` names with `make verus-install`, which refuses a download that does not match
`VERUS_SHA256`, and the toolchain `VERUS_TOOLCHAIN` names. A bump of the release changes all three
together.

`crates/core/src/spec.rs`, `verify/` and the two scripts have code owners in `.github/CODEOWNERS`.
A contract that is removed or weakened in the crate has to change `verify/contracts.txt` to pass,
and a definition a contract names lives in `spec.rs`, so either change needs an owner's approval.
The body a contract holds needs none, because Verus checks it.

## Running

Verus ships as a release archive that runs on the Rust toolchain `VERUS_TOOLCHAIN` names, through
rustup. On x86_64 Linux, `make verus-install` downloads and checks the release and prints the
directory to put on `PATH`. Elsewhere, install the release `VERUS_VERSION` names from
<https://github.com/verus-lang/verus/releases>. Then put `verus` on `PATH` or set `VERUS`, and run:

```sh
make check-verus
```

## What is not proved

- That the driver reaches untrusted bytes only through a gate. That is the type system and the
  counts in [specs/labels.md](../specs/labels.md).
- What a gate emits or records. `Policy::allow` and `Policy::deny` build their messages with
  `format!` and are `external_body`, so `read_trusted_content` is proved to refuse exactly when it
  should, and not proved to emit the event it does.
- What `read_trusted_content` returns. It returns a clone of the value, and Verus has no
  specification for the `Clone` that `Labelled` derives. A `Clone` written out with a contract is
  code with access to the private fields, which `check_labelled_impls` in the security-audit skill
  refuses.
- That a closure handed to `render_in_place` or `render_pair_in_place` decides nothing.
- What `read_output`, `promote_vetted` and `promote_vetted_picture` return. `SlotStore` is opaque
  to Verus, so their contract is the one on `endorsed_for_reading`, the question each asks before it
  reads the slot.
- That a release plan, an endorsement or the context integrity is what the session holds.
  `Policy` is opaque to Verus, so `precommitted`, `precommitted_into`, `consume_grant` and
  `context_integrity` are assumed to answer the uninterpreted `spec_precommitted`,
  `spec_precommitted_into`, `spec_granted` and `spec_context`.
- `Policy::record_note` and `Policy::promote_reference_for_read`. The first keeps the note in
  `Policy`, which is opaque to Verus, and the second reads the path out of `SlotStore`.
- Any gate without a row above.
