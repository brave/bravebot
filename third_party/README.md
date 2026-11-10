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
