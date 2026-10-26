# Smells and where each is checked

A code smell is a reason to look at code, and not always a defect. A 150-line function can read
well, and a 20-line one can hide a bad abstraction. Reviewers in the [maintenance
skill](SKILL.md) are told to call a candidate `cleanup` only when a person changing the code is
slowed or misled by it and the change that fixes that can be named.

The numbers are from a checklist of 80 Rust smells, in eight groups. Each is checked in one of
four ways, from the one that fails the build to the one that needs a person.

## Refused by the compiler, clippy or a check already in CI

These cannot reach `main`. Nothing here needs a lane.

| Smell | What refuses it |
|---|---|
| 3 too many parameters | `clippy::too_many_arguments`, under `-D warnings`. An allowance is a candidate in the `allowances` lane. |
| 14, 15 owned or `&Vec` where a borrow works | `clippy::ptr_arg` |
| 41 a lock guard held across `.await` | `clippy::await_holding_lock`. The tree has no async runtime. |
| 40 deeply nested types | `clippy::type_complexity` |
| 61, 63 unsafe code, `unsafe impl Send` | `#![forbid(unsafe_code)]` or `#![deny(unsafe_code)]` in each crate, so each use carries an allowance. |
| 67 `static mut` | `static_mut_refs`, denied in the 2024 edition |
| 77 dead code | rustc's `dead_code`. A `pub` item rustc cannot see is in the `unreferenced` and `test-only` lanes. |
| 78 formatting | `cargo fmt --all -- --check` |
| 56 a dependency for trivial work | `make check-deps` and [dependencies.md](../../../docs/best-practices/dependencies.md) |
| 58 feature flags | No crate in the workspace declares a feature. |

## Lanes of this skill

The scan finds candidates and a reviewer reads each one.

| Smell | Lane |
|---|---|
| 1, 4, 6, 7 long functions, doing several jobs, large files, god objects | `size`, `complexity` |
| 2 deep nesting | `complexity` |
| 8 boolean parameters | `complexity` |
| 5 duplicated logic | `duplicates` |
| 9 magic strings | `literals` |
| 13, 20 shared mutable state and interior mutability | `shared-state` |
| 27 string-typed errors | `error-types` |
| 51, 52, 53, 60 coupling and logic in `main.rs` | `coupling` |
| 69, 79 logic with no test, missing boundary tests | `untested` |
| 77 commented-out code, dead code behind an allowance | `stale`, `allowances` |
| 76 broad `#[allow]` | `allowances`, `clippy` |

## Clippy lints the `clippy` lane reads

These lints are off in `make check`. The lane runs them over the libraries and binaries and
groups the sites by lint and file. `CLIPPY_LINTS` in `maintenance.py` holds the list and a weight
for each, 3 for what the checklist rates high, 2 for medium and 1 for low.

| Smell | Lint |
|---|---|
| 21, 22, 23 `unwrap`, `expect`, `panic` | `unwrap_used`, `expect_used`, `panic` |
| 24 `let _ =` on a result | `let_underscore_must_use` |
| 62 unsafe blocks with no safety comment | `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block` |
| 64 narrowing casts, 65 slicing a string at a byte offset | `cast_possible_truncation`, `cast_sign_loss`, `string_slice` |
| 33, 8 invalid combinations of booleans | `struct_excessive_bools`, `fn_params_excessive_bools` |
| 1, 2 length and complexity | `too_many_lines`, `cognitive_complexity` |
| 11, 14 clones and owned arguments | `redundant_clone`, `implicit_clone`, `needless_pass_by_value` |
| 47 a lock held through work | `significant_drop_tightening` |
| 59 wildcard imports | `wildcard_imports` |
| 76 an allowance with no reason | `allow_attributes_without_reason` |

`redundant_clone` and `significant_drop_tightening` are `nursery` lints and are wrong at times.
`indexing_slicing` and `arithmetic_side_effects` are not read: they report most of the tree, and
a list that long is one nobody reviews.

## Judgement with no scan

A person decides these in review, against [best_practices.md](../../../docs/best_practices.md):
10 premature abstraction, 12 `Arc<Mutex<T>>` where ownership would do, 16 `Box<dyn Trait>`, 17
lifetime-heavy design, 18 allocation in a hot path, 26 `anyhow` across library interfaces, 28 a
default substituted after a failure, 29 an error logged and success returned, 30 inconsistent
error handling between modules, 31 `String` for everything, 32 a boolean where an enum fits, 34
and 35 what is `pub`, 36 and 37 generics and traits with one implementation, 38 naming, 39 missing
standard traits, 46 shared state against message passing, 54, 55 and 57 how crates are divided,
66 FFI contracts, 70, 71, 72 and 80 how tests are built, 73 documentation, 74 and 75 comments that
say what or disagree with the code.

43, 44, 45 and 50 are about tasks and apply here to threads: a thread whose handle is dropped, a
network call with no timeout, a channel with no bound. The `shared-state` lane counts threads and
channels and tells its reviewers to look for a dropped handle. A missing timeout has no scan.
68 is the rule this repository exists for, and `make check-security` and the security-audit skill
hold it.

41, 42, 48 and 49 are about async code and do not apply while the tree has no async runtime. They
need a lane when that changes.
