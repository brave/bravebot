# Kept in place

What an earlier split of a file said stays where it is. Those sentences are in closed issues, which
a reviewer reading a checkout does not see, so `prepare` copies the entry for a candidate's file
into that candidate's lines of the prompt in the `size`, `complexity` and `coupling` lanes. A
reviewer calls `keep` anything an entry covers unless it can say what changed since.

One `##` heading per file, the path as the scan prints it, then the issue, the items kept and the
reason that issue gave. A file with two entries has two headings. Add an entry when a split issue
says something stays; do not add one for a file that was split with nothing held back.

## crates/agent/src/turn.rs

Issue #2100. Every block that calls a label gate stays: `collect_delegates`, `answer_the_turn`,
`run_rounds`, the function that holds `render_in_place` and `declassify`, and in `one_turn` the
context-file reads and piped input, the attachment read, the streaming callback, the planner's own
turn, the whole tool-result presentation, and the tail that repeats `record_answer`. `one_turn` also
keeps building the policy. Reason: `labels.md` pins the `turn.rs` count of `Policy::present`,
`adopt_model_output`, `render_in_place`, `declassify` and `Labelled::trusted`, and a call that moves
to another file moves that file's count.

## crates/agent/src/workspace.rs

Issue #2097. The read, write, search and git methods stay, with `make_checkout`, `checkout_for`,
`checkout_in`, `checkout_status` and `read_checkout_file`. `Workspace::grep_shaped` stays: it
compares each line of the files a search walks with the pattern, which `labels.md` lists as a place
that reads untrusted bytes to decide something. Reason: moving them moves pinned `Labelled::new`,
`Labelled::declassify`, `Policy::render_in_place` and `Policy::read_planner_argument` sites, and
needs the review in `docs/development/reviewing-for-the-rule.md`.

## crates/cli/src/main.rs

Issue #2099. The `Ok(outcome)` arm of `run_and_report` stays inline with its logic unchanged: it
checks the reply against the output schema and picks the exit status from the result (CLI-28). The
dispatch `match` in `main()` stays, because it is a table. The tests stay in `main.rs`'s
`mod tests`, because the specs cite them by path in `verified-by:`. Reason: no decision about
labelled content moves, and messages, endings and exit statuses stay as they were.
