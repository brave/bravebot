# Build cache

A new checkout or git worktree starts with an empty `target/`, so its first build compiles every
dependency. [sccache](https://github.com/mozilla/sccache) keeps compiled crates outside `target/`
and reuses them across checkouts, which shortens that first build.

## Setup

```sh
brew install sccache
```

```toml
# ~/.cargo/config.toml
[build]
rustc-wrapper = "sccache"
```

This applies to every cargo build for the user. Once it is set, cargo fails with `could not execute
process sccache` on a machine where sccache is not installed.

sccache stores up to 10 GiB by default. To raise the cap, set it in its own config file, which on
macOS is `~/Library/Application Support/Mozilla.sccache/config`:

```toml
[cache.disk]
size = 42949672960   # 40 GiB
```

`sccache --show-stats` prints the hit counts and the cap. Leave incremental compilation on: cargo
applies it only to workspace crates, which this cache does not serve (see below).

## What it saves

Dependencies compile from `~/.cargo/registry`, the same path in every worktree, so a second
worktree reuses them. The workspace crates under `crates/` do not hit the cache, because a crate's
path is part of its compiler hash and each worktree has its own path. Linking is never cached.

The workspace crates are most of the work: in a full `cargo test --workspace --no-run`, they are
about 80% of the compile time, mostly their test targets. Cold builds in a fresh worktree on one
Apple Silicon Mac, before and with a warm cache:

| Command | Empty cache | Warm cache |
|---|---|---|
| `cargo build --workspace` | 46s | 31s |
| `cargo clippy --workspace --all-targets` | 36s | 29s |
| `cargo test --workspace --no-run` | 72s | 71s |

Expect a first build 15% to 30% shorter. The Docker checks (`check-msrv`, `check-windows`,
`check-linux`) copy the checkout into a container and do not use the host cache.
