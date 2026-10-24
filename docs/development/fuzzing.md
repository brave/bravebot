# Fuzzing

A panic in code that reads text or bytes from outside bravebot ends the session, because
`[profile.release]` sets `panic = "abort"`. A release build sets no `overflow-checks`, so a sum that
overflows wraps and the function carries on with a wrong number. Unit tests use inputs somebody
wrote. The fuzz targets in `fuzz/` run the same functions on inputs a generator wrote, and on bytes
a fuzzer grew from the ones that reached new code.

## The package

`fuzz/` is a [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) package with its own
`[workspace]` table and its own `Cargo.lock`. The root manifest does not list it, so `cargo build`,
`cargo clippy --workspace`, nextest and the cross-builds never compile libFuzzer's C++. The lockfile
began as a copy of the root `Cargo.lock`, so the crates under test build at the versions that ship.
Cargo added only `libfuzzer-sys`, `arbitrary` and what they need.

| Path | Holds |
|---|---|
| `fuzz_targets/<target>.rs` | One `fuzz_target!`. Its input type derives `Arbitrary`, so the type states what the input's bytes mean. |
| `src/` | What the targets share: generators, encoders and evaluators, with unit tests. |
| `dictionaries/<target>.dict` | Tokens the target's parser branches on. |
| `seeds/<target>/` | Committed inputs of at most 4 KiB, at most 100 for each target. |
| `regressions/<target>/` | Reproducers of crashes that were fixed. |

`corpus/` and `artifacts/` are where the fuzzer writes, and are not committed.

## The targets

| Target | Code | What an input is | Held to |
|---|---|---|---|
| `regex` | `agent::regex` | A pattern rendered from a syntax tree of the supported subset and a text, or raw pattern text | The engine agrees with an evaluator that reads the tree and shares no code with it, `matches` agrees with `matches_by_automaton`, a pattern over 1,000 characters or 50 groups deep is refused with the matching `PatternError`, and the answer does not depend on what the engine's buffers last held |
| `diff` | `agent::diff`, `agent::replace` | Two texts, or a text and edits that derive the second from it, or an edit | The lines a diff keeps are the same in both texts, its rows account for every line, its hunks put `before` back byte for byte, and `replace` refuses what its documentation says it refuses |
| `event_stream` | `bedrock::eventstream::FrameDecoder` | Raw bytes, or frames with correct checksums, the last of them damaged, and in either case cut into reads | Any cutting into reads gives the same events, a frame that claims an impossible size or ends in a wrong checksum is refused, and an undamaged stream gives the events its frames hold |
| `sse` | `aichat::protocol::SseDecoder`, `ChatChunk`, `StreamAccumulator` | Lines of known meaning, or raw bytes, cut into reads, with a limit from a few bytes to a few kibibytes | Any cutting into reads gives the same payloads, the decoder holds no more than its limit, a line over the limit ends the decoding, and the accumulator holds what the chunks say |

`regex` and `sse` need three functions that only the targets use from outside their crates.
`Regex::matches_by_automaton` skips the literal shortcuts. `SseDecoder::with_limit` builds a decoder
with a small limit, and was private to the module that builds the default one. `SseDecoder::held`
reads how much a decoder holds. At the limit every caller gets, 16 MiB, no input a fuzzer makes
could reach the bound. All three are `#[doc(hidden)]`.

## Running

The targets need a nightly toolchain with the `rust-src` component, and cargo-fuzz. `make check-fuzz`
prints the command for whichever is missing. The toolchain is named once, as `FUZZ_TOOLCHAIN` in the
`Makefile`, and the commands below read it from there. `bravebot-config`'s build script refuses to
build without credentials unless it is told a binary configured at run time is acceptable, which
the `make` targets say for themselves and a command typed here has to:

```sh
TC=$(make -s fuzz-toolchain)
export BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1
rustup toolchain install "$TC" --profile minimal -c rustfmt -c clippy -c rust-src
cargo install cargo-fuzz --locked
```

```sh
make fuzz TARGET=regex                       # one target for FUZZ_SECONDS, 20 by default
make fuzz TARGET=diff FUZZ_SECONDS=600
make check-fuzz                              # lint, build, replay, then every target for FUZZ_SECONDS
```

`make check-fuzz` runs `cargo fmt --check`, clippy with `-D warnings`, the unit tests of `fuzz/src`,
`cargo fuzz build -a` for every target, a replay of `seeds/` and `regressions/` with `-runs=0`, and
then each target. It stops at the first failure. It is not part of `make check-all` or
`make check-affected`, which run without a nightly toolchain.

Targets are built with debug assertions and overflow checks (`-a`) and AddressSanitizer, so an
arithmetic site that wraps in a release build is reported when an input reaches it. Every
`cargo fuzz` command takes `-a` for the same reason: an input that an overflow check stopped does not
reproduce in a build without it. Runs use `-timeout=10 -rss_limit_mb=2048`, and `-max_len=4096`
except `diff`, which reads two texts and gets `65536`. A target that has a dictionary in
`dictionaries/` is given it.

## The lockfile

`fuzz/Cargo.lock` began as a copy of the root `Cargo.lock`, so the crates under test build at the
versions that ship. It does not follow the root lock on its own. `make bump-version` refreshes the
version of each bravebot crate in it. After a dependency in the root lock changes, bring it up to
the root lock again:

```sh
cp Cargo.lock fuzz/Cargo.lock
cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1 > /dev/null
```

Cargo keeps every version the copy names and adds only what the fuzz package needs.

## Adding a target

1. Write `fuzz_targets/<target>.rs` with an input type that derives `Arbitrary`. Put a generator,
   an encoder or an evaluator that more than one target would use in `src/`, with unit tests that
   state what it must produce, since a wrong evaluator reports a defect in code that is right.
2. Add a `[[bin]]` entry to `fuzz/Cargo.toml` with `test = false`, `doc = false` and `bench = false`,
   and add the target to `FUZZ_TARGETS` in the `Makefile`. `make` refuses to run while the two
   lists differ.
3. State the properties from the code's documentation, a spec clause, or a type's contract, and name
   which one each assertion comes from. An assertion that restates the implementation finds nothing.
4. Add `dictionaries/<target>.dict` where the target reads text with tokens the parser branches on.
5. Create `seeds/<target>/` and `regressions/<target>/` (with an empty `.gitkeep`, since git keeps no
   empty directory), and put seeds in `seeds/<target>/`.
6. Run each assertion's fault: change the code under test so the property is false, and check that
   the target stops. A property that no input can fail is not checked by running the target for longer.

## Seeds and the corpus

A grown corpus holds many inputs that reach the same code. Reduce it before promoting any of it:

```sh
cargo +$TC fuzz cmin -a regex                  # keeps the inputs that each reach something new
```

Keep at most 100 inputs of at most 4 KiB for a target, choosing the ones that between them reach the
most code, and put them in `seeds/<target>/`. `make check-fuzz` replays what is there. The encoding
of an input type that derives `Arbitrary` is not one to write by hand, so seeds come from a corpus.

`cargo +$TC fuzz coverage -a <target>` writes coverage data that `llvm-cov` reads, from the
toolchain's `llvm-tools` component (`rustup component add llvm-tools --toolchain "$TC"`). Use it to
see whether a target reaches the code it was written for, such as the parsing past the prelude
checksum in `event_stream`, or the automaton in `regex`.

## When a target finds something

1. Reproduce with `cargo +$TC fuzz run -a <target> <artifact>` and reduce with
   `cargo +$TC fuzz tmin -a <target> <artifact>`.
2. Fix it with a unit test in the crate that owns the code, which fails before the fix
   ([best-practices/tests.md](../best-practices/tests.md#TS-003)), so the case runs on every platform
   CI covers. Add the reproducer to `regressions/<target>/`.
3. File it with the [issue-poster](../../agents/agents/issue-poster.md) definition, as a `bug` with
   the area of the code. The issue names the target and the first frames of the trace. Where the
   input steered an effect or reached a context closed to it, the finding is about the guarantee and
   takes `security`, `needs-security-review` and a `severity`
   ([labelling-issues.md](labelling-issues.md#security)), and goes through HackerOne
   ([SECURITY.md](../../SECURITY.md)) without the reproducer attached. Attach the reproducer to
   any other.
4. A timeout or an out-of-memory stop is a finding. A failed property is a finding against the
   documentation or clause it came from. Where the property was wrong, correct it in the harness
   and say why in the commit.

## What the targets do not check

The `event_stream` target checks that a frame claiming more than 16 MiB is refused. It does not
measure how much the decoder allocates, since that needs a global allocator, which is `unsafe`.
