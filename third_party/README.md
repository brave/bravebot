# third_party

Crates kept in the tree because bravebot changes them or must control what they start. Each is
listed in `[patch.crates-io]` in the root `Cargo.toml` and excluded from `[workspace] members`, so
the workspace lints, `cargo fmt --all` and `cargo clippy --workspace` do not apply to it.
`make check-reviewdog` does not count findings in these crates either; the organization's required
`security` check still scans them ([checks.md](../docs/development/checks.md)).

## crossterm

crossterm 0.29.0 from crates.io (MIT, `third_party/crossterm/LICENSE`), checksum
`d8b9f2e4c67f833b660cdb0a3523065869fb35570177239812ed4c905aeff87b`.

Differences from the published crate:

- `Cargo.toml` has no `[[example]]` targets and no `[dev-dependencies]`, and the `examples`,
  `docs`, `.github` and changelog files are not copied.
- `src/terminal/sys/unix.rs`: `size()` returns the error of the window-size query, and `tput_value`
  and `tput_size`, which started `tput` by bare name, are removed (SANDBOX-30, #3802). A stray pair
  of parentheses that rustc warns about is dropped from `window_size()`.
- `src/event.rs`, `src/event/read.rs`, `src/terminal/sys/*.rs`: `push_input()` hands input another
  reader took off the terminal to the event queue ahead of what is still unread, as bytes on Unix
  and as `INPUT_RECORD`s on Windows, and returns how much of it was no event.
  `supports_keyboard_enhancement_within()` gives the keyboard query a timeout in place of the fixed
  two seconds. On Windows the event reader shares the mouse buttons it last saw held with
  `push_input()`, so a mouse record handed back is read as the console reader would read it. This
  is what keeps keys typed while the terminal is asked about itself at startup (INPUT-52, #3846).

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
- `src/picker.rs`, `src/picker/cap_parser.rs`: `Picker::from_query_stdio_keeping_input()` reads
  the replies to its queries on the calling thread, stops at the timeout, and returns what it read
  that is not part of a reply: bytes on Unix, and on Windows the console's input records, read
  only while the console holds some, through a `CONIN$` handle it closes when it is done. The
  published crate read on a thread of its own on Windows, discarded everything else it read and
  left that handle open (INPUT-52, #3846).

To update it, replace `src/` and `Cargo.toml` with those of the new release, apply the changes
listed above, check that the crate's source starts no program (`grep -rn Command src build.rs`), and run
`make check-deps`. A raise needs the dependency audit of
[DEP-001](../docs/best-practices/dependencies.md).
