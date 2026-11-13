# Verification

[Verus](https://github.com/verus-lang/verus) checks that a function meets its contract for every
input. A test or a fuzz target runs some inputs and finds a counterexample if it meets one; a proof
says there is none. Contracts sit on the code that ships, so an edit that breaks one fails
`make check-verus`.

## What has a contract

| Item | Contract |
|---|---|
| `Integrity::meet`, `Confidentiality::join` | Each returns the pessimistic answer |
| `Label::degrades_to` | It is the axis-by-axis order taint travels in, which `crates/core/src/spec.rs` proves a partial order with `(T,priv)` and `(U,pub)` incomparable |
| `Label::is_trusted`, `Label::is_public` | Each reads its axis |
| `Labelled::label`, `Labelled::declassify` | Each returns the label or the value held |
| `Labelled::into_trusted` | The value comes out exactly when the label is `(T,pub)`, and otherwise the labelled value comes back unchanged |
| `Labelled::relabel` | It succeeds exactly when the new label is a degradation, and keeps the value |
| `Policy::refuse_untrusted`, `Policy::read_trusted_content` | Each refuses exactly when the label is untrusted |

`verify/mutations.txt` lists a defect for each, and `make check-verus` requires Verus to refuse
every one.

`verify/history.rs` is a copy of three fixes from the history, 3facf14a, e0cd1903 and 20ce6086,
with a contract each, written before the crates they are in carried contracts. Verus refuses each
under the `before_<commit>` cfg that restores the code before the fix. A copy can drift from the
code, so a fix there moves into its crate as a contract when the crate is set up for Verus.

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
- A module that holds a contract imports `vstd::prelude::*` behind `cfg(verus_keep_ghost)`.
- A type a contract reads takes `#[cfg_attr(verus_keep_ghost, verus_verify)]`, and
  `derive(Structural)` there too where a contract compares it with `==`.
- Definitions a contract names, and proofs, go in `crates/core/src/spec.rs` inside `verus! { }`.
  Write them from what the code is for, not by copying the body: a spec copied from the code proves
  the code equals itself.
- A private field is read in a contract through a `closed spec fn` written beside the type, as
  `spec_label_of` and `spec_value_of` are in `crates/core/src/value.rs`. A spec function exists only
  for Verus, so it opens nothing in a build, and it is not a function of the type's own.
- A function Verus cannot read, such as one building a `String` with `format!`, takes
  `verus_verify(external_body)`, and then its contract is assumed rather than proved. Keep such
  functions to the ones a reviewer can check by eye, and say in the pull request which they are.

Then:

1. Add a mutation to `verify/mutations.txt` that breaks what the contract states, and run
   `make check-verus`. The crate has to verify and the mutation has to be refused.
2. Run `python3 contrib/check-verus-contracts.py --write`, which records the contract in
   `verify/contracts.txt`.
3. Say in the pull request what each new contract states and which mutation shows it can fail.

## What the checks hold

- `make check` and `make check-scripts` run `contrib/check-verus-contracts.py`, which needs no
  toolchain. It fails when a contract in `crates/` differs from `verify/contracts.txt`, so a
  contract that is removed, added or weakened shows in the diff. CI runs it in the Security job.
- `make check-verus` runs Verus. Each crate in `CRATES` in `contrib/check-verus.py` has to verify,
  each mutation has to be refused, and `verify/history.rs` has to verify and be refused under each
  `before_<commit>` cfg.

CI does not run `make check-verus` yet. Until it does, a change to a function with a contract runs
it before it is pushed, and the pull request says that it passed.

## Running

Verus ships as a release archive that brings its own Rust toolchain through rustup. Install the
release `VERUS_VERSION` in the `Makefile` names, put `verus` on `PATH` or set `VERUS`, and run:

```sh
make check-verus
```

## What is not proved

- That the driver reaches untrusted bytes only through a gate. That is the type system and the
  counts in [specs/labels.md](../specs/labels.md).
- What a gate emits or records. `Policy::allow` and `Policy::deny` build their messages with
  `format!` and are `external_body`, so `read_trusted_content` is proved to refuse exactly when it
  should, and not proved to emit the event it does.
- That a closure handed to `render_in_place` or `render_pair_in_place` decides nothing.
- Any gate without a row above.
