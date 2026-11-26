# Verification

[Verus](https://github.com/verus-lang/verus) checks that a function meets its contract for every
input. A test or a fuzz target runs some inputs and finds a counterexample if it meets one; a proof
says there is none. Contracts sit on the code that ships, so an edit that breaks one fails
`make check-verus`.

## What has a contract

| Item | Contract |
|---|---|
| `Integrity::meet`, `Confidentiality::join` | Each returns the pessimistic answer |
| `Capability::output_label`, `Policy::observe`, `Policy::before_fetch`, `Policy::before_search` | Whatever a capability observes is labelled untrusted: a fetch and a search public, every other read private, and a write has no label |
| `CapabilitySet::contains`, `CapabilitySet::within`, `CapabilitySet::revoke_mcp_call`, `Policy::before_capability` | A capability is used exactly where the run holds it. A set narrowed by a parent holds only what both hold, so a delegate and a turn addressed to a definition hold nothing their parent lacks, and withdrawing a server's grant takes that grant and no other |
| `Label::degrades_to` | It is the axis-by-axis order taint travels in, which `crates/core/src/spec.rs` proves a lattice with `(T,priv)` and `(U,pub)` incomparable, in which the taint of two labels is their least upper bound |
| `Label::is_trusted`, `Label::is_public` | Each reads its axis |
| `Labelled::label`, `Labelled::declassify` | Each returns the label or the value held |
| `Labelled::into_trusted` | The value comes out exactly when the label is `(T,pub)`, and otherwise the labelled value comes back unchanged |
| `Labelled::relabel` | It succeeds exactly when the new label is a degradation, and keeps the value |
| `Policy::refuse_untrusted`, `Policy::read_trusted_content` | Each refuses exactly when the label is untrusted |
| `policy::shown_to_the_planner` | `Policy::present` shows the planner content exactly where it is trusted and not a picture, and every other value reaches it as a reference |
| `Context::absorb`, `Context::hold`, `Context::fresh` | A turn's context starts trusted and public. Integrity only meets and confidentiality only joins, so nothing shown later restores what was already lost. `Policy::absorb` meets `spec_context` with what was observed |
| `Grants::spend`, `Grants::issue` | An endorsement is spent only for exactly the tool, field and value it was issued for, and a spent one is gone, so it cannot authorise a second action or another value. `Policy::consume_grant` spends from `spec_grants` |
| `Labelled::new`, `Labelled::trusted`, `Label::new` | Each builds the value and label it is given, `trusted` at `(T,pub)` |
| `Policy::read_planner_argument`, `Policy::promote_confined_read` | Each passes exactly when the argument is public and the planner's context has met nothing untrusted, and `promote_confined_read` returns `(T,pub)` |
| `Policy::declassify`, `Policy::declassify_into_place` | Each releases exactly the slots the release plan named before any content was seen, `declassify_into_place` only to the destination named with the slot |
| `Policy::declassify_into_workspace`, `Policy::declassify_written_into_workspace`, `Policy::declassify_checkout_into_workspace` | Each returns the value at public with the integrity it had, so a release never raises integrity |
| `Policy::endorsed_for_reading` | `read_output`, `promote_vetted` and `promote_vetted_picture` hand a slot to the planner only where it is the kind the gate serves and a single-use endorsement for it was issued |
| `Policy::promote_a_tool_list`, `Policy::promote_a_server_move` | Each passes exactly when a single-use endorsement for the server's alias was issued, and returns `(T,pub)` |
| `Policy::adopt_edited_body` | It adopts a body a person edited exactly when the proposed body was `(T,pub)`, and records the saved body at `(T,pub)` |
| `Policy::record_answers` | It records a reply exactly when the questions were trusted, at the questions' own label |
| `Policy::label_user_command_output`, `Policy::label_user_configuration`, `Policy::label_kept_fact` | Each gives the first label its provenance names: `(T,priv)` for a command the person typed, `(T,pub)` for their configuration, and trusted at the confidentiality the fact was kept at |
| `Plan::can_be_remembered` | It allows a line exactly when everything that decides what each step does is in the entry `a` records, as `spec::plan_is_its_entries` states over every field of `Plan` and `Step`: no file written or read by name, no reference fed in, no assignment, and no route that opens a file. A route that opens none (a stream sent to `/dev/null`, or `2>&1`) is allowed because the entry holds it: `spec::command_is` compares routes, so the entry made for `sh check.sh > /dev/null` does not cover `sh check.sh` |
| `Plan::can_be_remembered_as` | It allows a request exactly when its plan can be remembered and it asked for no scope, did not ask to run with no profile, and writes no protected path. `Plan::vouch` makes the only `Vouch`, and `Policy::remember_vouch` records only from one |
| `policy::run_reason` | Private input, a requested scope, a write, an assignment, and a line outside the root that no entry names are always put to a person, and so is a line an `ask` or `deny` rule matches. A line goes unasked only on the answer that covers it, and a fed reference only on an `allow` rule or the audited table |
| `policy::output_label` | Output is trusted only on a proof from the audited table or a vouch for every step, never for a line that asked for a scope or to run unconfined, and never on a vouch for a line carrying an assignment or fed untrusted content. The opaque label it is handed is read as untrusted whatever its integrity |
| `policy::write_reason`, `WriteReason::asks` | A write to a destination nobody has read or to a protected path is always put to a person, whatever a rule says, and so is one an `ask` or `deny` rule matches. A write goes unasked only on an `allow` rule, or on the trust map where it answers: never into a path nobody has vouched for, and never with untrusted data into a trusted path |
| `policy::fetch_reason`, `FetchReason::asks` | A fetch goes unasked exactly where an `allow` rule names its host |
| `policy::mcp_reason`, `McpReason::asks` | A call to a server's tool is always put to a person where the server's declaration asks about every call or the arguments are private, whatever a rule or a standing answer says, and so is one an `ask` or `deny` rule matches. It goes unasked only on an `allow` rule or, where no rule matched, on an approved plan or the person having said to stop asking |
| `policy::search_reason`, `SearchReason::asks` | A search goes unasked only where an `allow` rule for the family answers and the planner's context is public. A context that holds private content is always put to a person, whatever a rule says, and so is a search an `ask` or `deny` rule matches |
| `trust::meet_of_rules`, `policy::weakest_name` | A subtree or a file asked about under several names is trusted only where every rule bearing on it, and every name, answers trusted. An undecided boundary, a name nothing answers, and a file with no name count as untrusted, so a rule can only lower the answer (`spec::a_meet_of_rules_is_trusted_only_where_every_rule_is`) |
| `trust::weakest_decision`, `trust::weaker`, `trust::strength` | Where several spellings of one name hold a decision, the one that grants least decides: a distrust before an undecided boundary before a trust |
| `Standing::authorizes`, `Grant::allows` | A request from a device is allowed exactly where the host holds the device as paired with a grant that covers the operation, as `spec::device_verdict` states: a device with no record and a revoked device are never allowed whatever grant they held, a `read` grant covers reading and never control, and a `control` grant covers both. The answer is read from what the person did on the host alone, with no input from the request (`spec::a_request_is_allowed_only_for_a_paired_device_whose_grant_covers_it`) |
| `Scope::covers` | A session's scope is inside the ceiling a device was approved for exactly where none of its twenty-one fields is past it, as `spec::field_widened` states for each: a granting list holds nothing the ceiling's did not, a restricting list holds all of it, an undecided path of the ceiling's may be replaced by a distrust and no other path is undecided, an `ask` rule may be replaced by a `deny`, the sandbox reads and writes no more, the permission mode is no further along its order, `reads_stay_in_workspace` is not turned off, and the other booleans are not turned on, automatic vetting in neither direction. `spec::a_scope_covers_itself` proves a scope is inside the ceiling it is. The list of which fields went past is built by `flagged` and checked by the tests, and the contract states only that it is empty exactly where nothing did |
| `trust::covers_bytes` | A trust-map key covers another segment by segment: the root covers every key, the empty key every key that is not a full path, and any other key itself and what is beneath it after a `/`, so `src` never covers `srcfoo`. `covers_spelled` asks it of the two keys' bytes. Folding case and Unicode canonical equivalence (`fold_case`, `fold_simple`, `canonical_form`) happen before it and are not proved |
| `TrustedPrograms::covers_every_step` | Every step a plan runs, including each step inside a `$(...)`, has an entry equal to it in file, start path, arguments, routes and tree |
| `Remembered::covers`, `RememberedStep::matches`, `Shape::matches` | A remembered line covers a plan exactly where the lines are joined the same way and each step matches in name, file, start path, assignments, routes and arguments, with a number slot admitting only a number, and the step carries no expansion |
| `permissions::ruling_of` | A matching `deny` decides over every other rule, an `ask` over an `allow`, and an `allow` only where nothing restricts |
| `permissions::pipeline_decision` | A `deny` on any stage denies a pipeline, an `ask` on any stage asks, and a pipeline is allowed only where it has stages and every one is allowed |
| `Policy::before_fetch_rules`, `Policy::before_search_rules`, `Policy::before_mcp_call_rules`, `Policy::before_read`, `Policy::refuse_denied_write` | Each refuses exactly where a `deny` rule covers the action: the host, the search, the call, the path read, and for a write the path edited or read. `Policy::rules_for_*` are assumed to return what `Permissions` says, which matches patterns Verus does not read |
| `credentials::hex_value`, `credentials::sextet_of` | Each gives the value of a hexadecimal or base64 character exactly where the byte is one, and nothing for any other byte |
| `credentials::escape_at`, `credentials::percent_bytes` | Every offset read is inside the text, a whole `%` escape is always decoded, including one that ends the text, and the output is never longer than the input and is shorter where an escape was decoded |
| `credentials::base64_bytes` | Fewer than eight bits are held between characters and the bits held are below two to the power of how many are held, so no shift is by 32 or more, no mask underflows, and there is at most one byte out for each character in |

`verify/mutations.txt` lists a defect for each, and `make check-verus` requires Verus to refuse
every one.

`contrib/check-verus-sweep.py` makes the defects nobody listed. It changes one token at a time in
the body of each function Verus proves against a contract: `==` and `!=`, `&&` and `||`, `true`
and `false`, a dropped `!`, `<` and `<=`, `>` and `>=`, `Integrity::Trusted` and `Untrusted`,
`Confidentiality::Public` and `Private`, and `Ruling::Allow` and `Ask`. A change Verus verifies
survives, and `make check-verus` fails unless `verify/survivors.txt` lists it with the reason it is
acceptable. It fails as well where a listed change no longer survives.

Most contracts here state one direction: what a gate always asks about or refuses, and when output
may be trusted. They do not state that a gate lets through what it may. A change that makes a gate
stricter meets such a contract and survives, and its entry in `verify/survivors.txt` says so. A
unit test is what pins that a gate is not stricter than it has to be. A survivor that lets more
through is a contract that does not say what it is for, so strengthen the contract instead of
listing it.

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
  `invariant` and `decreases` the same way, on the loop. Verus checks a `requires` only at a
  caller it verifies, so a function with a caller Verus does not verify checks its input and
  states the result in `ensures` instead. The ledger lists each `requires` as an assumption, so a
  new one shows in the diff with its callers for a reviewer to check.
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
  attribute, an `assume` or `admit`, an `assume_specification`, or a contract with a `requires`. Verus verifies a crate with
  any of these, so the ledger is what shows a reviewer each one. CI runs it in the Security job.
- `make check-verus` runs Verus. Each crate in `CRATES` in `contrib/check-verus.py` has to verify,
  each mutation has to be refused, each change `contrib/check-verus-sweep.py` makes has to be
  refused or listed in `verify/survivors.txt`, and `verify/history.rs` has to verify and be refused
  under each `before_<commit>` cfg.

CI's `Verus` job runs `make check-verus` on a pull request that changes `crates/core/`, `verify/`
or the scripts that run them, on the merge queue's branch, and on `main`. It installs the release
`VERUS_VERSION` names with `make verus-install`, which refuses a download that does not match
`VERUS_SHA256`, and the toolchain `VERUS_TOOLCHAIN` names. A bump of the release changes all three
together.

A pull request that weakens what these hold is flagged for a person. After CI runs on a pull
request, the `Proof weakened` workflow runs `contrib/check-verus-weakened.py` against the merge base
and lists in its job summary every contract the change removes or changes, assumption it adds,
mutation it removes and survivor it adds, counting a line the ledger already holds once more, and
every edit to `spec.rs`, `verify/history.rs` or a script that runs the checks. The list is a
`Proof weakened` entry in the pull request's checks, neutral where it lists anything and success
where it does not, so it blocks nothing. A weakening labels the pull request `proof-weakened`. An
edit to one of those files weakens nothing by itself, and is listed apart and labelled
`proof-tooling-changed`. Either asks the owners of `verify/` to review, and a label whose list is
empty comes off. It runs from
the default branch with that branch's copy of the script, on the commit GitHub says CI ran on, so a
pull request can change neither the script nor which pull request is labelled, and its code is
read with `git show` and never run.
It blocks nothing, since a changed contract can be a stronger one and an assumption added can be
the price of a proof, so the pull request says why each item is right. `make check-verus-weakened`
prints the same list here. A function that should have a contract and was given none changes no
line of the ledger, and this does not see it.

`crates/core/src/spec.rs`, `verify/` and the scripts have code owners in `.github/CODEOWNERS`, so
GitHub asks an owner to review a change to them. A contract that is removed or weakened in the
crate has to change `verify/contracts.txt` to pass, and a definition a contract names lives in
`spec.rs`, so either change reaches an owner, and so does a survivor listed in
`verify/survivors.txt`. The `main` ruleset does not require that owner's approval, so the
`proof-weakened` label is what tells a reviewer to read it. The body a contract holds needs no
owner, because Verus checks it.

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
  `spec_precommitted_into`, `spec_granted` and `spec_context`. `Policy::absorb` is assumed to meet
  `spec_context` by handing the observation to its `Context`, whose `absorb` is proved, and
  `consume_grant` to spend from `spec_grants` by handing the call to its `Grants`, whose `spend` is
  proved.
- `Policy::record_note` and `Policy::promote_reference_for_read`. The first keeps the note in
  `Policy`, which is opaque to Verus, and the second reads the path out of `SlotStore`.
- Any gate without a row above.
