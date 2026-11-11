# third_party

Crates kept in the tree because bravebot changes them or must control what they start. Each is
listed in `[patch.crates-io]` in the root `Cargo.toml` and excluded from `[workspace] members`, so
the workspace lints, `cargo fmt --all` and `cargo clippy --workspace` do not apply to it.

## crossterm

crossterm 0.29.0 from crates.io (MIT, `third_party/crossterm/LICENSE`), checksum
`d8b9f2e4c67f833b660cdb0a3523065869fb35570177239812ed4c905aeff87b`.

Differences from the published crate:

- `Cargo.toml` has no `[[example]]` targets and no `[dev-dependencies]`, and the `examples`,
  `docs`, `.github` and changelog files are not copied.
- `src/terminal/sys/unix.rs`: `size()` returns the error of the window-size query, and `tput_value`
  and `tput_size`, which started `tput` by bare name, are removed (SANDBOX-30, #3802). A stray pair
  of parentheses that rustc warns about is dropped from `window_size()`.

To update it, replace `src/` and `Cargo.toml` with those of the new release, apply the changes
listed above and in the commits that touch `third_party/crossterm/src`, and run `make check-deps`.

## ratatui-image

ratatui-image 11.1.0 from crates.io (MIT, `third_party/ratatui-image/LICENSE`), checksum
`9593d10c61ec2007f429d1fafc4f4a19fecdbae4d06b531a2d505227fb82f5ea`.

Differences from the published crate:

- `Cargo.toml` has no `[[bin]]`, `[[example]]` or `[[bench]]` targets and no `[dev-dependencies]`,
  and the `src/bin`, `examples`, `benches`, changelog and CI files are not copied.
- `src/picker.rs`: `detect_tmux_and_outer_protocol_from_env()` no longer starts
  `tmux set -p allow-passthrough on`, which the published crate ran by bare name with the whole
  environment when `TERM` starts with `tmux` or `TERM_PROGRAM` is `tmux` (SANDBOX-30, #3853). The
  detection of tmux and the wrapping of graphics sequences for it are unchanged. Without that
  command, tmux drops the sequences until the person sets `allow-passthrough` themselves.

To update it, replace `src/` and `Cargo.toml` with those of the new release, apply the changes
listed above, check that the crate's source starts no program (`grep -rn Command src build.rs`), and run
`make check-deps`. A raise needs the dependency audit of
[DEP-001](../docs/best-practices/dependencies.md).
