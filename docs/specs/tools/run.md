---
id: RUN
title: run
status: normative
governs:
  - crates/agent/src/exec.rs
  - crates/agent/src/remembered.rs
  - crates/agent/src/scrub.rs
  - crates/config/src/scrub.rs
  - crates/core/src/command.rs
  - crates/core/src/programs.rs
  - crates/core/src/policy.rs
  - crates/core/src/remembered.rs
  - crates/tui/src/confirm.rs
guards:
  - symbol: Policy::read_output
  - symbol: Policy::remember_command
  - symbol: TrustedPrograms::trust
reads_a_step_without_keying:
  - crates/core/src/policy.rs::plan_lines
  - crates/core/src/policy.rs::read_proven
documented-by:
  - docs/website/docs/reference/tools.md
  - docs/website/docs/security/permissions.md
---

## Scope

`run`, its labels, and the two ways a person can change them. Shell mode is [shell-mode.md](../shell-mode.md) and is not an
instance of this: it is not a tool and the planner cannot reach it. What a program may reach once it
is running is [../sandboxing.md](../sandboxing.md)'s.

## Why a program is admissible when a shell is not

A shell string is destination and payload at once, so there is nothing in it a person could
approve on its own, and a parser that tried to work out what it means would be racing a shell it
does not control. An argument list has no such problem. This is the distinction to hold onto: it
is not that command execution turned out to be acceptable after all, it is that the exclusion was
about shell strings and an argv vector is not one.

## Clauses

<a id="RUN-1"></a>
### RUN-1: `run` takes one command line, and the execution path takes argv

```
run { command: "git log --oneline -50 | sed -n 1,10p" }
```

The line is compiled into an ordered plan of stages, each a resolved binary and a literal argument
vector, and the plan is what executes. `; rm -rf /` inside quotes is one argument and stays one,
because the only thing that ever split the line was the compiler and it already ran. What the
grammar accepts, what it refuses, and why a compiled plan is not an interpreted string are
[command-line.md](command-line.md).

**The planner's execution path stays argv-only** and must never build a command line. It and shell
mode are separate modules, so a change to one cannot quietly become a shell for the other.

`verified-by: bravebot_agent::exec::a_metacharacter_in_an_argument_stays_one_argument`
`verified-by: bravebot_agent::exec::a_redirection_in_an_argument_writes_no_file`
`verified-by: bravebot_agent::exec::stages_are_chained_so_one_feeds_the_next`
`verified-by: bravebot_agent::exec::a_single_stage_returns_what_it_printed`
`verified-by: bravebot_core::policy::a_line_with_no_steps_is_refused`

<a id="RUN-2"></a>
### RUN-2: the plan is routing and must be endorsed by a person

Programs, arguments, the directory a plan runs in, and the files it writes must be `(T,pub)`.
Untrusted text never becomes one. The endorsement is bound to that exact plan, so it cannot be reused for a different one: not
for the same steps joined differently, not for the same steps writing somewhere else, and not for
the same steps in another directory.

`verified-by: bravebot_core::policy::a_plan_without_an_endorsement_is_refused`
`verified-by: bravebot_core::policy::an_endorsement_does_not_authorise_a_plan_that_writes_elsewhere`
`verified-by: bravebot_agent::exec::a_stage_runs_the_binary_it_was_resolved_to`
`verified-by: bravebot_agent::exec::a_pipeline_with_missing_resolutions_does_not_run`

<a id="RUN-3"></a>
### RUN-3: stdin is content and may be untrusted

The planner names a quarantined reference and the policy layer supplies the bytes, so `sed` and `awk`
work on a file nobody vouched for without the planner or the driver ever reading it. A stage that
reads stdin and was given none receives nothing, never the terminal.

`stdin_ref` is the field, and it names a reference rather than a path: the planner may not have the
filename, and the name it does have is one the driver minted. The reference is resolved before
anybody is asked, because its label is what decides whether there is a prompt at all, and the bytes
stay wrapped until the endorsement is consumed. They go to the step at the head of the line and no
other: fed once, so `cat && cat` is the second `cat` reading what the line gives it.

**One source per descriptor.** A call may name `stdin_ref` or write a `<` redirection and not both;
the two are the routes [RUN-4](#RUN-4) names to one place, and whichever lost would have been
dropped into a run that looked like it had worked. The refusal is made twice, where the call is read
and again where the bytes would be written. `background: true` is refused with it for the same
reason: nothing is waited for there and nothing is fed either.

**Why.** This is the point of the split: both trusted and untrusted data reach real tools, and
only the routing part has to be trustworthy.

`verified-by: bravebot_agent::exec::a_stage_that_reads_stdin_is_given_nothing_rather_than_the_terminal`
`verified-by: bravebot_agent::exec::bytes_supplied_for_standard_input_reach_the_first_stage`
`verified-by: bravebot_agent::exec::bytes_supplied_for_standard_input_reach_one_stage_and_no_other`
`verified-by: bravebot_agent::exec::more_bytes_than_a_pipe_holds_are_fed_without_deadlocking`
`verified-by: bravebot_agent::exec::a_program_that_reads_none_of_what_it_was_fed_is_not_a_failed_run`
`verified-by: bravebot_agent::exec::a_line_naming_a_file_for_standard_input_cannot_also_be_fed_bytes`
`verified-by: bravebot_agent::turn::a_quarantined_reference_is_fed_to_a_program_the_planner_may_not_read`
`verified-by: bravebot_agent::turn::a_private_reference_fed_to_a_vouched_line_is_still_put_to_a_person`
`verified-by: bravebot_agent::turn::a_background_line_cannot_be_fed_a_reference`
`verified-by: bravebot_agent::turn::a_line_naming_a_file_for_standard_input_cannot_also_name_a_reference`
`verified-by: bravebot_tui::confirm::a_run_prompt_names_the_reference_it_would_be_fed`

<a id="RUN-4"></a>
### RUN-4: output is untrusted and private by default, and nothing inferred changes that

| | Label | Gate |
|---|---|---|
| Program and arguments | `(T,pub)` | a person approves the exact argv |
| Standard input | may be untrusted | a person approves when it is private |
| Standard output and error | `(U,priv)` | quarantined |
| …for a command a person vouched for | `(T,priv)` | RUN-7 |
| …for a plan that proves what it read | the meet over its read set, private | [command-line.md](command-line.md) |

A program may print bytes an earlier stage read out of a file an attacker wrote, so `(U,priv)` is
the only label that holds without knowing what ran. Nothing a caller, a stage, or the planner can
declare changes it. Two things can establish a better first label, and neither is a declaration: a
person, in one of the two ways below, which is an assertion they take responsibility for, and a
proof about one program's option surface, checked by hand against that program's full option list
and covering the exact arguments given.

Standard input reaches the second row by two routes, and the row governs both. The policy layer
may supply the bytes of a quarantined reference, which arrive with that reference's label, and a
`<` redirection names a file the run opens itself. A file's bytes are the user's own data whatever
the trust map says about the path, so the second route is always private and always meets the gate.

**Both roads to a trusted label are met with what was fed in.** A program prints what it was
given, so a vouched-for `sed` over a fetched page prints the page: neither an assertion about a
program nor a proof about its option surface reaches `(T,priv)` for a line whose standard input
came from a reference nobody vouched for. Reading it the other way would make a vouch for one
filter a way of laundering any quarantined document into the planner's context, which is the
guarantee this repository exists for. The endorsement is bound to it too, so an answer given for a
line fed one thing is not redeemable for the same steps fed another.

`verified-by: bravebot_core::command::a_file_redirected_into_a_program_is_private_input`
`verified-by: bravebot_core::command::a_redirection_on_a_later_step_is_private_input`
`verified-by: bravebot_core::command::a_plan_that_feeds_a_program_nothing_releases_nothing`
`verified-by: bravebot_core::policy::output_of_a_line_nobody_vouched_for_is_untrusted_and_private`
`verified-by: bravebot_core::policy::output_of_a_line_whose_every_step_was_vouched_for_is_trusted_and_still_private`
`verified-by: bravebot_core::policy::one_unvouched_step_makes_the_whole_lines_output_untrusted`
`verified-by: bravebot_core::policy::a_vouched_line_fed_content_nobody_vouched_for_prints_untrusted_output`
`verified-by: bravebot_core::policy::a_vouched_line_fed_content_the_user_vouched_for_still_prints_trusted_output`
`verified-by: bravebot_core::policy::a_private_reference_fed_to_a_vouched_line_is_put_to_a_person`
`verified-by: bravebot_core::policy::an_endorsement_does_not_authorise_the_same_plan_fed_something_else`
`verified-by: bravebot_core::command::a_plan_fed_a_reference_encodes_apart_from_the_same_plan_fed_nothing`

A file-dependent output label must also account for overlapping effects. If shared file state
changes between the run gate and output capture, apart from the command's own guarded effect
entries, that output is untrusted. This includes background output when polled. This conservative
check can quarantine output after an unrelated file decision; it never broadens command approval.

`verified-by: bravebot_agent::turn::foreground_redirection_quarantines_live_reads_and_all_endings`
`verified-by: bravebot_agent::tools::an_ended_job_revalidates_its_file_proof_before_releasing_output`

<a id="RUN-5"></a>
### RUN-5: every run asks, unless every stage was vouched for, remembered, ruled on, or proven

There is no *declared* read-only category. `foo --bar` might write to disk and nothing here can
tell, and a stage declaring itself harmless only helps if the declaration is honest. Four things
may answer the question, and nothing else: a person having answered it before, in this session, for
this exact command; that person having asked at a prompt for their answer to one exact command line
to last past the session, which [RUN-19](#RUN-19) specifies; a rule the person
wrote down in advance, which
[permissions.md](../permissions.md) governs and which stops the asking without raising any label;
and the audited table in [command-line.md](command-line.md) establishing that these exact arguments
write nothing and read only paths the user vouched for. Never a property of the argv this system
worked out for itself, never a declaration by a stage, never anything derived from what a program
printed.

**Why.** An unprompted write is worse than an unwanted prompt, so nothing that could be wrong about
a write may answer the question. The middle two are the same authority the first road has, exercised
about a line the person read and recorded where they can read it back, and both grant strictly less
than a vouch: they stop the question and leave every label where it was. An entry in the table is a
claim checked by hand against one program's full option list, which is why it may, and it is narrow
for the same reason: anything it does not fully recognise asks.

`verified-by: bravebot_core::policy::a_command_nobody_vouched_for_is_put_to_a_person`
`verified-by: bravebot_core::policy::a_line_remembered_past_the_session_is_not_asked_about`
`verified-by: bravebot_core::policy::a_rule_the_user_wrote_in_advance_answers_the_run_prompt`
`verified-by: bravebot_core::policy::an_allow_rule_stops_the_prompt_and_does_not_trust_what_the_command_prints`
`verified-by: bravebot_core::policy::a_vouched_command_is_not_asked_about_again`
`verified-by: bravebot_core::policy::one_unvouched_step_puts_the_whole_line_to_a_person`
`verified-by: bravebot_core::policy::a_line_that_only_reads_vouched_for_paths_does_not_ask`
`verified-by: bravebot_core::policy::a_line_reading_an_unvouched_path_still_asks`
`verified-by: bravebot_core::policy::one_step_nothing_can_account_for_makes_the_whole_line_opaque`

<a id="RUN-6"></a>
### RUN-6: private input asks every time, whatever is vouched for

Untrusted input is fine, since carrying bytes decides nothing. Private input hands the user's data
to a program, and that releases it somewhere this policy stops governing. Trusted-but-private asks
too, and `a` is not offered for those runs at all.

**Why.** Vouching for what a file contains is not consenting to send it somewhere, and trusting a
command is not consenting to hand it the user's data.

**Why `a` is withheld rather than narrowed.** What the key records is a program and its exact
arguments, and a `<` redirection is in neither: an entry made while one file was redirected in
would cover the same program fed any other file. Withholding the key is what keeps the entry
honest about what it covers, and the refusal is made twice: once where the prompt is drawn, and
again where an answer is acted on, since an invariant about what the trusted list may hold does
not rest on a drawing.

**A `>` redirection is withheld on the same grounds.** An entry records no destination any more
than it records a source, so an entry made at a write prompt would cover this program and these
arguments with the redirection gone, which is a line the person never read. The two directions are
one rule, and this is the direction where the cost of offering the key is plainest: a line that
writes is asked about every time whatever is recorded ([RUN-19](#RUN-19)), so `a` cannot stop the
next prompt for *this* line, and the only thing it could ever grant is the bare one. The person did
read the program and the arguments the entry would hold, which is what makes this narrower than a
source; what they did not read is the line that entry covers. The refusal is made twice, as above.

`verified-by: bravebot_core::policy::private_input_asks_even_for_a_vouched_line`
`verified-by: bravebot_agent::cmdline::an_input_redirection_is_private_input`
`verified-by: bravebot_agent::turn::a_line_that_reads_a_file_is_not_remembered_however_it_is_answered`
`verified-by: bravebot_agent::turn::a_line_that_writes_is_not_remembered_however_it_is_answered`
`verified-by: bravebot_tui::confirm::a_run_reading_a_file_offers_no_standing_permission`
`verified-by: bravebot_tui::confirm::a_run_that_releases_private_data_offers_no_standing_permission`
`verified-by: bravebot_tui::confirm::a_run_writing_a_file_offers_no_standing_permission`
`verified-by: bravebot_tui::confirm::pressing_always_at_a_private_input_prompt_grants_nothing`

<a id="RUN-7"></a>
### RUN-7: vouching grants two things together, and the prompt asks for both

```
  y run it    a always    n don't    ctrl-c stop the turn
```

This is the row a run prompt would draw on its own. [RUN-19](#RUN-19) specifies a longer lifetime,
the key that grants it and the label `a` carries beside it, and its row replaces this one wherever a
run prompt is drawn.

`a` grants, in these terms:

1. the command runs again unasked, side effects and all;
2. what it prints is `(T,priv)`, so the planner reads it instead of a reference.

The second is a **human assertion, not an inference**. Nothing establishes that a vouched command
is side-effect-free or that its output is free of influence, and nothing tries: `git log` prints
commit messages whoever contributed wrote. It is trusted for exactly the reason a directory in the
trust map is trusted, which is that the user said so. Do not reach for a stronger justification,
and do not let anything else mint an entry.

`verified-by: bravebot_tui::confirm::a_run_prompt_asks_for_the_side_effects_and_the_output_together`
`verified-by: bravebot_core::policy::a_turn_inherits_what_the_session_vouched_for`

<a id="RUN-8"></a>
### RUN-8: an entry is keyed by resolved path, exact arguments, and the tree it was given in

`git log` says nothing about `git push`, and nothing about `git log --all`. `$PATH` and aliases
decide what a name means, so an assertion must not follow a name onto a different binary. Never
widen an entry to a program alone. In a pipeline **every** stage must be vouched for or the whole
output is untrusted, since an unvouched stage in the middle is a transformation nobody answered
for and its output is what the next stage read.

An entry also records **where** the command runs, and grants RUN-7's two things in that one
directory and nowhere else. The tree is the third thing a run prompt shows, beside the resolved
binary and the argv ([CMDLINE-12](command-line.md#CMDLINE-12)), and an entry holding two of the
three would record less than the question asked: `sh check.sh` designates a different file in every
tree it is read in, so an answer given about the one in `sub/` is not an answer about a `check.sh`
that appears at the root afterwards, and `git clean -fd` is a different proposition in two different
trees. `git log` pointed at a vendored dependency is asked about however often `git log` was vouched
for at the root, and what it prints is `(U,priv)`, because no entry names `vendor/dependency`.

**One directory, and not the tree beneath it.** An entry given in `sub/` grants nothing in
`sub/nested/`. Matching a prefix would put this same hole one level down, since the relative
argument that designates a different file at the root designates a different file in a subdirectory
too.

**The canonical absolute path, as the only spelling.** `sub`, `./sub` and a symlink pointing at
`sub` are one tree, and three keys for it would be three chances for a prompt somebody has already
answered to appear again. Resolution happens where the I/O does, before a plan exists, for the
reason the program is the resolved path.

**The path itself, and never a rendering of it.** A path is bytes, and not every sequence of bytes is
text. `to_string_lossy` maps every byte it cannot read onto one replacement character, so two files
whose names differ only in such bytes render to one string, and an entry keyed on that string covers
both of them while the execution path spawns whichever of them the plan named: an answer about a file
nobody was shown. The same holds of the tree, whose own key already turns on a spelling. So the
binary an entry names and the tree it was given in are both compared as paths, and both are written
by the path's own bytes wherever a path has no spelling as text. A rendering is for the screen, where
a person reads a name rather than
matching on one, and a recorded path that *is* a rendering designates no file: it covers nothing
rather than covering every file it could have meant, which is what [RUN-9](#RUN-9) and
[RUN-19](#RUN-19) say of the two records that outlive the prompt.

Nothing else a person settled in advance reaches a tree of its own: the trust map's relative rules,
a rule in a settings file, and a line remembered past the session ([RUN-19](#RUN-19)) are spelled
against the workspace root, and a remembered line records no tree at all. So a line running outside
the root is asked about unless an entry names the tree it runs in.

An entry says nothing about the **environment** either, because nothing in it records a `NAME=value`
assignment written in front of a program. So it grants neither of RUN-7's two things for a line
carrying one: `LD_PRELOAD=./evil.so git log` is asked about however often `git log` was vouched for,
and what it prints is `(U,priv)`. An assignment decides what a program loads and reads before its own
arguments are looked at, so the line is a different proposition from the one the person read, and
what it printed was written by whatever that assignment brought in. `a` is not offered for such a
line at all, and the refusal is made twice, once where the prompt is drawn and again where an answer
is acted on: an entry made there would be a bare entry, covering the same program under no
assignment, so the list would end up holding something nobody was shown. The directory is not the
precedent for widening the key here: a tree has no other spelling, and an assignment has one
already, which is the paragraph below. The
question is put before a rule in a settings file is consulted, as
[permissions.md](../permissions.md) requires: a rule is matched against the program and its arguments
run together, a rendering an assignment is not in, so no rule anybody could write tells the two lines
apart.

**A key is built by destructuring the step, never by reading its fields.** The three paragraphs above
were each written after the same defect: a key holding less than the prompt displayed, so one answer
covered a line nobody read. The environment was missing, then the tree, then the path's own bytes.
Each was corrected where it was found, and nothing stopped the next function from being written the
same way, because reading two fields of a step and stopping there is not something a compiler has any
reason to report. So every function that builds a key, an entry, or an encoding of a step opens by
destructuring it, naming a field `_` where it is deliberately left out, and a field added to the step
stops the build at each one until somebody decides whether the key holds it. The same holds of the
code that applies a step to a process: a field that changes what runs and is in no key is a line
running differently from the one that was approved.

A compiler reports the field added to the step, and `make check-security` reports the function newly
written to read one field at a time, which is the half a compiler cannot see. Neither is a habit
somebody has to hold, and the second fails the same pull request that introduces the function.

A function may read a step field by field for something that is not a key, and the two that do are
named under `reads_a_step_without_keying` above rather than left to be recognised. `plan_lines` builds
the rendering a `deny` rule is matched against, which is the program and its arguments run together
and deliberately not the whole step. `read_proven` refuses a step carrying an assignment or a route
before it reads anything else, so the fields it goes on to read are the only ones such a step has. A
third one is an edit to this list, which is the point: admitting one is something somebody reviews.

**A known cost.** `NO_COLOR=1 cargo test` and `RUST_LOG=debug ./demo` are ordinary work, and they are
asked about every time, in this session and in the next. The answer is the spelling that puts the
assignment where a person reads it and an entry can hold it: `env NO_COLOR=1 cargo test` is a program
called `env` with three arguments, so a vouch for it covers that line and no other, and the person
approving it saw the assignment in the argv they approved. What this clause refuses is a line whose
meaning is not in its argv, not the setting of a variable.

`verified-by: bravebot_core::policy::vouching_for_one_command_does_not_cover_another_of_the_same_program`
`verified-by: bravebot_core::policy::vouching_does_not_follow_a_name_onto_a_different_binary`
`verified-by: bravebot_core::policy::a_vouched_line_is_asked_about_when_it_runs_outside_the_root`
`verified-by: bravebot_core::policy::output_of_a_vouched_line_run_outside_the_root_is_untrusted`
`verified-by: bravebot_core::policy::a_vouched_line_is_asked_about_when_no_root_is_known`
`verified-by: bravebot_agent::turn::a_vouched_line_is_asked_about_again_when_a_directory_is_named`
`verified-by: bravebot_core::policy::an_entry_given_outside_the_root_does_not_cover_the_same_line_at_the_root`
`verified-by: bravebot_core::policy::output_at_the_root_of_a_line_vouched_for_outside_it_is_untrusted`
`verified-by: bravebot_core::policy::an_entry_given_outside_the_root_grants_both_things_in_that_tree`
`verified-by: bravebot_core::policy::an_entry_does_not_cover_a_directory_below_the_one_it_names`
`verified-by: bravebot_core::policy::a_remembered_line_is_still_asked_about_outside_the_root_when_nothing_is_vouched_for`
`verified-by: bravebot_core::programs::vouching_in_one_tree_says_nothing_about_the_same_command_in_another`
`verified-by: bravebot_agent::turn::a_line_vouched_for_outside_the_root_is_asked_about_again_at_the_root`
`verified-by: bravebot_agent::turn::a_symlinked_spelling_of_the_vouched_tree_is_the_same_entry`
`verified-by: bravebot_agent::turn::a_second_spelling_of_the_vouched_tree_is_the_same_entry`
`verified-by: bravebot_tui::confirm::a_run_prompt_names_the_tree_the_entry_would_be_given_in`
`verified-by: bravebot_core::policy::a_vouched_line_carrying_an_environment_assignment_is_asked_about`
`verified-by: bravebot_core::policy::output_of_a_vouched_line_carrying_an_environment_assignment_is_untrusted`
`verified-by: bravebot_tui::confirm::a_run_carrying_an_environment_assignment_offers_no_standing_permission`
`verified-by: bravebot_tui::confirm::a_run_that_is_private_and_carries_an_assignment_gives_both_reasons`
`verified-by: bravebot_agent::turn::a_line_carrying_an_environment_assignment_is_not_remembered_however_it_is_answered`
`verified-by: bravebot_core::programs::two_binaries_differing_only_in_unrenderable_bytes_are_different_programs`
`verified-by: bravebot_core::programs::two_trees_differing_only_in_unrenderable_bytes_are_different_trees`
`verified-by: bravebot_core::policy::a_vouch_does_not_cover_a_binary_that_only_renders_the_same_way`
`verified-by: bravebot_core::command::a_spelling_names_the_path_it_was_taken_from`
`verified-by: bravebot_core::command::a_text_spelling_holding_a_replacement_character_names_no_path`
`verified-by: bravebot_core::command::a_path_that_really_holds_a_replacement_character_still_names_itself`

<a id="RUN-9"></a>
### RUN-9: the vouched list belongs to the session

Empty at the start of every session, written into the session record, restored by `--resume`,
never inherited by a fresh session in the same directory. `/status` lists every entry it holds
rather than a count of them, and names the tree each one covers ([RUN-8](#RUN-8)): two entries for
one command in two directories are two grants, and a report drawing them as one line twice says
nothing about which of the two is held. A line somebody asked to be remembered past the session is a
separate record holding less, which [RUN-19](#RUN-19) governs, and it puts no entry in this list.

A tree inside the project is written into the record relative to it and comes back under the
directory the resumed session works in, as a rewind's paths and the trust map's rules are
([SESSION-22](../sessions.md#SESSION-22)); a tree outside the project is written in full, there
being nothing to write it against. So a checkout that is moved or renamed keeps its entries, and an
unrelated checkout standing where it used to be inherits none of them. Records are found by the
directory a session ran in, which a second checkout at that path inherits, so a tree written in full
would be an entry answering in a tree nobody vouched for: the hole this clause's own key exists to
close, one checkout out.

A record written before an entry held a tree restores as one given at the workspace root, which is
the only place such an entry could ever have been spent. Nothing else is migrated: reading it any
other way would either widen a permission nobody gave or drop one they did. An entry whose recorded
binary or tree is a rendering rather than a path restores as nothing, for the reason
[RUN-8](#RUN-8) gives: it designates no file, and every path it could have meant would be a grant
nobody gave. That entry alone is dropped and not the record holding it, the rest of the session's
answers being the same person's.

**Why.** The same reason the trust map belongs to a session. Its effect is invisible until a prompt
does not appear, so it has to be readable back.

`verified-by: bravebot_core::policy::a_fresh_policy_vouches_for_no_command`
`verified-by: bravebot_core::policy::a_line_remembered_past_the_session_vouches_for_nothing`
`verified-by: bravebot_tui::status::every_vouched_command_is_listed_however_many_there_are`
`verified-by: bravebot_tui::status::the_report_names_the_tree_each_vouched_command_runs_unasked_in`
`verified-by: bravebot_session::sessions::an_entry_recorded_without_a_tree_comes_back_scoped_to_the_root`
`verified-by: bravebot_session::sessions::a_tree_inside_the_project_is_written_down_relative`
`verified-by: bravebot_session::sessions::a_tree_written_down_relative_comes_back_under_the_resumed_root`
`verified-by: bravebot_session::sessions::a_binary_no_rendering_can_show_comes_back_as_itself`
`verified-by: bravebot_session::sessions::an_entry_whose_recorded_binary_is_a_rendering_vouches_for_nothing`

<a id="RUN-10"></a>
### RUN-10: the vouched-for list is not an allowlist and must never become one

It never decides what may run. A command nobody vouched for still runs after a prompt, nothing is
refused for being absent, and the set is empty at the start of every session. Programs are not
enumerated and not confined: they run with the access the user's shell would give them, because
`git push` needs `~/.ssh` and the set of programs someone might ask for cannot be listed in
advance.

Do not add an allowlist and treat it as the safety property. What holds is the label on the
output, not a belief about the binary. The audited table in [command-line.md](command-line.md) is
not one: a program absent from it is neither refused nor confined, only asked about, and what the
table establishes is what an output may be labelled rather than what may run.

`verified-by: bravebot_core::policy::a_command_nobody_vouched_for_is_put_to_a_person`
`verified-by: bravebot_core::policy::a_fresh_policy_vouches_for_no_command`
`verified-by: bravebot_agent::turn::an_approved_run_executes_and_the_user_saw_what_it_was`
`verified-by: bravebot_agent::exec::the_users_own_environment_still_reaches_a_program`

<a id="RUN-11"></a>
### RUN-11: a run has a wall-clock limit, and reaching it ends the run rather than failing it

A pipeline is given 300 seconds unless the call named its own deadline, which it may do up to a
ceiling it cannot exceed; [CMDLINE-13](command-line.md#CMDLINE-13) is that bound. When the deadline
runs out the stages are killed, and what they printed
before that is collected and returned exactly as it is for a pipeline that ended by itself, under
the label RUN-4 gives it. The stop is reported as structure, a duration on the result, so a
caller says which of the two happened without reading a byte of what was printed. Collecting after
a kill must be abandonable: a killed stage can leave a child holding the write end of the pipe, so
the run may not wait on that pipe reaching its end, and keeps whatever had been read by then.

**Why.** The limit is for the program that does not end, and the commonest such program is doing
exactly what was asked: a server told to serve a page serves it, prints as it goes, and never
exits. Returning the failure alone threw that account away and left a program that hung
indistinguishable from one that was working.

A program *meant* to keep running is asked for differently, and [RUN-15](#RUN-15) is how. The limit
is the answer for the program that hangs; applied to a server it meant the only way to start one
was to have it killed at the deadline, with no moment at which it was up and could be used.

**Not a safety property.** A stage that finishes inside the limit is no safer than one that
outstays it, and nothing may be inferred about what a program did from the fact that it stopped in
time. This is a bound on futility: a program that never returns holds the turn open with nothing
to show for it. Being cut short neither raises nor lowers the label on the output, which is
RUN-4's to decide.

`verified-by: bravebot_agent::exec::a_pipeline_stopped_at_the_limit_still_returns_what_it_printed`
`verified-by: bravebot_agent::exec::a_pipeline_that_ends_by_itself_is_not_marked_stopped`
`verified-by: bravebot_agent::exec::a_grandchild_holding_the_pipe_does_not_hang_the_run`

<a id="RUN-12"></a>
### RUN-12: a program is not handed this agent's own credentials

The environment a stage receives is the one this process holds, less the credentials this agent
authenticates to its backend with and with the session's own directory named in it
([TRUST-17](../trust-map.md#TRUST-17)). Every stage, not only the first. Removed rather than
emptied, so a program that distinguishes an unset variable from a blank one sees what a machine that
never held the credential sees.

**Why.** A person approving a run reads the argv, the resolved binary and the directory. The
environment is not among those, so a credential travelling alongside them is granted without having
been seen, and "run `git log`" is approved as an inspection of the repository. A subprocess has no
use for these: a program the planner chose is doing what somebody approved, not authenticating as
this agent.

**The rest of the environment stays, and that is not an oversight.** `run aws s3 ls` and `run gh pr
list` are ordinary requests, and no rule matching variable names can tell one of those from an
exfiltration. Withholding by guesswork would trade a claim that holds exactly for one that mostly
holds. What a person is told is the truth about the remainder: a run has the access their own shell
has, which is what the prompt says every time. A person who wants a name of their own withheld may
list it, and a list of names can only ever take something away. That list is read once when the
process starts, so editing it applies to the next session rather than to a run already in flight.

**Not a confinement mechanism, and it must not be read as one.** A program that reaches the network
is unconfined and unpoliced, so it can send anything it can read: a file, the workspace, a credential
of the user's own. Those requests are the program's own and do not pass the gate in
[network-egress.md](../network-egress.md), which governs what this process sends rather than what a
program it started sends. What closes here is the narrow part of the gap, the credentials a person
could not have been shown at the prompt and had no way to withhold. The rest of what they hand over is
still handed over. Nothing is established about what the program then does, and the label on the
output is unaffected.

A line a person typed themselves is not this, and keeps the whole environment: it is meant to behave
as their own terminal does, and nothing else about it is gated either.

`verified-by: bravebot_agent::exec::this_agents_own_credentials_do_not_reach_a_program_it_runs`
`verified-by: bravebot_agent::exec::no_stage_of_a_pipeline_sees_this_agents_credentials`
`verified-by: bravebot_agent::exec::the_users_own_environment_still_reaches_a_program`
`verified-by: bravebot_agent::exec::the_plumbing_a_program_needs_is_still_inherited`
`verified-by: bravebot_agent::exec::the_filtering_can_be_switched_off_by_its_documented_spelling_only`
`verified-by: bravebot_config::scrub::this_agents_credentials_are_withheld_without_being_configured`
`verified-by: bravebot_config::scrub::nothing_of_the_users_own_is_withheld_by_guesswork`
`verified-by: bravebot_config::scrub::a_name_from_the_settings_file_is_withheld_as_well`

<a id="RUN-13"></a>
### RUN-13: the caller is told how the run ended, whichever way the label went

Every result carries the driver's sentence about how the run ended, said from the exit codes and
the clock: that it exited 0, which stages did not and with what code, or that it outstayed the
limit and was stopped. It stands in front of what the program printed, and it is there in the same
words where the output is quarantined and the caller holds a reference to it instead.

**Why.** A program's own bytes do not say whether it did what it was asked. A test run prints much
the same lines whether it passed or failed, a command that fails silently prints nothing at all,
and a caller left to infer the verdict from the output either re-runs everything or believes
whatever the last line implies. It stands in front of the output because a build log's verdict is
not always in the lines a reader gets to.

**Why it holds for quarantined output.** The exit status is structure and not content, exactly as a
line count is: it was read off the process and never out of a byte the program printed, so telling
a planner about it puts nothing in its context that a program chose. Withholding it would leave the
one case where the output is least useful, a reference the planner may not read, as the one case
where it also cannot tell success from failure.

`verified-by: bravebot_agent::turn::the_planner_is_told_how_a_run_it_may_read_ended`
`verified-by: bravebot_agent::turn::the_planner_is_told_how_a_run_it_may_not_read_ended`

<a id="RUN-14"></a>
### RUN-14: a quarantined result says what would lift the quarantine

Where the output could not be shown, the planner is also told how to see this result and how to
stop being asked: `read_output` puts this one to the user, and a person vouching for every stage of
the exact command makes what it prints visible from then on. It is also pointed at `read_file` for
a file. Only where a command produced the result: a quarantined read carries no advice about
`read_output` or about vouching for a command nobody ran. The advice about vouching is left out where
a record already stops the asking for that exact line ([RUN-19](#RUN-19)), since no prompt will
return there for a person to answer, and `read_output` is then the whole of what is said.

Where permissions are bypassed with no screening asked for, neither half is true: nobody is shown
what `read_output` releases ([MODE-4](../permission-modes.md#MODE-4)), and a run the mode approved
vouches for no program, so no vouch stops an asking that does not happen. The planner is told
instead that `read_output` hands the output back as text it can read, and, on a run's result, that
`read: true` returns it in the result that ran it ([RUN-22](#RUN-22)). A job's result and the account
the turn gives of an ended job say the same of `read_output` and nothing of `read`, which is an
argument of `run` alone. None of it names the mode, as no mode but plan mode is named to the planner
([MODE-3](../permission-modes.md#MODE-3)): a model told nobody is watching has been handed a reason
to be bolder.

**Why.** [RUN-4](#RUN-4) is about who answered for the command, not about programs being
unreadable, and a planner that reads it the second way stops running them. One did: told once that
`sed` on a source file could not be shown to it, it spent the rest of a session reading files
singly through `read_file`. It never called `read_output`, which exists for this and would have
answered it in one call, and it never asked the user to vouch for anything either. That cost it the
batching it had been using and cost them a turn that produced nothing. The same sentence was added
to a quarantined *read* for the same reason, and the run path never got it.

**This changes no label.** What is said is what [RUN-7](#RUN-7) already provides for, and saying it
is not inferring it: the planner still cannot vouch for anything, and a person still answers.

`verified-by: bravebot_agent::turn::a_quarantined_run_says_what_would_make_it_visible`
`verified-by: bravebot_agent::turn::a_quarantined_read_says_nothing_about_vouching_for_a_command`
`verified-by: bravebot_agent::turn::a_quarantined_result_from_a_remembered_line_says_nothing_about_vouching`
`verified-by: bravebot_agent::turn::an_unscreened_unattended_run_that_does_not_ask_to_read_hands_back_a_reference_and_says_how_to`
`verified-by: bravebot_agent::turn::what_a_job_printed_says_how_to_read_it_where_nobody_is_asked`
`verified-by: bravebot_agent::turn::what_an_ended_job_printed_says_how_to_read_it_where_nobody_is_asked`

<a id="RUN-15"></a>
### RUN-15: a pipeline may be left running, and the turn that started it ends it

`background: true` starts the line and does not wait for it, handing back a job name. The name is
the driver's own, minted here and looked up in this module's own map, so it is trusted and public
and the planner may use it as routing. `job_output` reports what the job has printed since the last
look, whether it has ended, and kills it on request.

Every gate is the one a foreground run passes, at the same point and in the same order: the rules,
the person's approval, and the label the output will carry are all settled before anything starts.
Being left running is not a reason to ask for less.

**One pipeline, and no redirection.** A line with `&&` or `||` decides where to go next by waiting
on the part before it, and nothing waits here; a redirection names a destination the background has
no reader for. Both are refused rather than half-honoured, and so is a redirection that opens no
file: what a background run cannot honour is a route, whether or not it names one.

**The turn owns it.** Dropping the handle kills the pipeline, so a job cannot outlive the turn that
started one. A background program still running after its turn ended would be an effect nobody is
watching, nobody is being asked about, and nobody can stop.

**Ended means the account is complete.** A job is reported as ended once every step has exited and
every pipe has reached its end, which are not the same moment: a step can print and exit with its
output still in the pipe, unread. A caller told a job ended stops asking, so reporting it at the
first of the two hands over an account missing its last lines and nothing ever hands over the rest.
Waiting for the pipes is bounded, and one a step's own child is still holding is abandoned exactly
as [RUN-11](#RUN-11) abandons it: what had been read by then is kept, and a pipeline whose steps
have all exited is never reported as still running.

**Why.** A program meant to keep running is what [RUN-11](#RUN-11)'s limit cannot serve. Its own
rationale names the case: a server told to serve serves, prints as it goes, and never exits. Waiting
for one and killing it at the limit leaves no moment at which it is up and can be used, so the turn
that started a server could never talk to it.

`verified-by: bravebot_agent::exec::a_background_pipeline_reports_what_it_printed_while_it_is_still_running`
`verified-by: bravebot_agent::exec::a_background_pipeline_that_finishes_says_so_and_reports_its_code`
`verified-by: bravebot_agent::exec::a_background_pipeline_reported_as_ended_has_all_of_its_output`
`verified-by: bravebot_agent::exec::a_killed_background_pipeline_keeps_what_it_printed`
`verified-by: bravebot_agent::exec::background_stages_are_chained_so_one_feeds_the_next`
`verified-by: bravebot_agent::exec::dropping_a_background_pipeline_kills_it`
`verified-by: bravebot_agent::exec::a_background_pipeline_with_missing_resolutions_does_not_start`
`verified-by: bravebot_agent::turn::a_background_server_is_still_running_when_the_next_call_is_made`
`verified-by: bravebot_agent::turn::a_background_command_must_be_one_pipeline`
`verified-by: bravebot_agent::turn::a_background_line_is_refused_for_any_redirection_it_carries`
`verified-by: bravebot_agent::turn::a_refused_background_run_starts_nothing`
`verified-by: bravebot_agent::turn::asking_about_a_job_that_does_not_exist_says_so`

<a id="RUN-16"></a>
### RUN-16: what a background job printed keeps the label its plan was given

The label is fixed by [RUN-4](#RUN-4) when the job starts and kept with it, rather than worked out
again when the output is read. Each look reports what is new since the last one, counted in bytes.

**Why the label is kept rather than recomputed.** What a person has vouched for can change during a
turn, and a pipeline started before that must not have its output relabelled because of it. A second
derivation of the same thing is a second answer waiting to disagree with the one the trail recorded.

Reporting only what is new is bookkeeping about how much has been handed over, never a comparison of
what was printed: nothing here reads a byte of it.

`verified-by: bravebot_agent::turn::what_a_background_job_printed_is_quarantined_like_any_other_output`
`verified-by: bravebot_agent::exec::a_background_pipeline_does_not_see_this_agents_credentials`

<a id="RUN-17"></a>
### RUN-17: a look at a job may wait for it, inside the turn that owns it

`job_output` takes `wait_seconds`, and a call that gives it comes back at the first of four things:
output arriving that the caller has not been handed, the job ending, the wait running out, or the
turn being cancelled. Which of the four it was is not reported separately, because the answer already
says it: how many new lines there are, whether the job has ended, and how long the wait actually
lasted.

**Between one second and ten minutes, and a value outside that is refused.** [RUN-11](#RUN-11)'s
deadline is clamped instead, and the difference is what a caller can tell afterwards. A run whose
deadline was shortened still ends, and the answer says how long it took, so nothing is hidden. A wait
that was shortened comes back with silence, and silence carries no length: a caller that asked about
ten minutes and was quietly given one reads the same nothing either way and reports it as ten
minutes of nothing.

**The bound is the wait's own, not the deadline's.** The two numbers happen to match today and are
still separate, because they answer separate questions. A deadline bounds how long a pipeline may
run; a wait bounds how long one look may sit watching one. Deciding that a build may take longer is
not deciding that a single look may sit there longer, and a wait bounded by whatever the deadline
bounds today would move whenever that decision was made. Both of the wait's numbers are quoted to the
planner, in the tool's description and in the refusal, so they are pinned to what a caller is told
rather than to each other.

**The window is named, to the planner and not only on a screen.** A wait that ends early because
output arrived is otherwise indistinguishable from one that sat out its bound, so the answer says how
many seconds were spent, and says that nothing is watching now. Without it, nothing new is read as a
standing account of the job rather than as an account of some seconds of it. It is said as structure
on the result, beside the exit codes and the clock, because the words a person watching reads reach a
screen and stop there.

**A job still running is reported as running and not as stopped.** [RUN-11](#RUN-11)'s stop is
something that happened to a pipeline; a look is not. Reporting a look at a live job the way a
deadline is reported tells the planner the job is over, which is exactly the wrong thing to tell one
that is waiting for the job to print again.

**A job that has ended is reported by the codes its steps exited with.** Not as having succeeded, and
not as whatever the look then did to it. A caller that waited for a build to finish and was told it
exited zero reports a red build as green, and where the output is quarantined that one sentence is the
whole account of the build the planner ever gets. The codes are structure this driver kept about a
pipeline it started, so saying them reads nothing of what was printed.

**This still cannot outlive the turn.** [RUN-15](#RUN-15) is unchanged: the handle is dropped at the
end of the turn and the pipeline dies with it. A wait is a way to spend part of one turn watching,
not a way to be told about something later, and the tool says so where it offers it. Watching that
has to survive a turn is [loop.md](../loop.md) and nothing here.

**Nothing of the output is read.** Both things the wait watches are counts this driver kept about a
pipeline it started: how many bytes have arrived, and which steps have exited. That is the
bookkeeping [RUN-16](#RUN-16) already provides for, and the byte count is only ever compared against
itself. A program does therefore decide when a wait returns by choosing when to print, which is
exactly what a caller asking to be told about new output asked for, and the bytes themselves still
reach anybody only under the label the plan was given.

**What is new is counted per pipe.** A pipeline has one pipe for its standard output and one for each
stage's standard error, and what a look hands back composes them with the output first. A single
offset into that composition is therefore wrong the moment a line arrives on standard output after
something has printed on standard error: every byte of the error text moves further along, the offset
names a place in the middle of text the caller was already shown, and the bytes it was waiting for sit
before that place and are never handed over at all. One offset per pipe, and each pipe compared
against itself. A character the pipe has only half delivered is held back until the rest of it
arrives, rather than handed over as a replacement character that the real one would then never
replace.

**The token is checked every pass, and no pass blocks.** A bound running to ten minutes and a person
who has changed their mind are the whole reason: cancelling should not mean sitting through the rest
of somebody else's `tail -f`. Checking often is only worth as much as the longest pass, so nothing
inside a pass waits on anything. In particular a wait asks only whether the steps have exited, and
does not also give the pipes their moment to catch up with them: that moment belongs to the look that
settles the final account, is spent after the wait has returned, and is outside the bound. Spending it
inside would carry a wait past the seconds it was given and would sit there without looking at the
token.

**Why.** Without a wait, watching a job costs a whole turn per look. The planner calls `job_output`,
is told nothing has happened, has to answer, and is asked the same question again, so a program that
prints once a minute costs a round trip a minute and the user reads a running commentary of nothing.
It is also what a bounded "tell me when this changes" needs in order to be answerable at all: one
call that covers a window, rather than a snapshot the planner is tempted to report as an answer about
the window.

`verified-by: bravebot_agent::exec::waiting_for_more_returns_when_the_job_prints_rather_than_at_the_bound`
`verified-by: bravebot_agent::exec::waiting_for_more_lasts_its_bound_where_a_job_that_has_printed_says_nothing_further`
`verified-by: bravebot_agent::exec::waiting_for_more_returns_when_the_job_ends_without_printing`
`verified-by: bravebot_agent::exec::a_cancelled_wait_for_more_comes_back_without_waiting_out_its_bound`
`verified-by: bravebot_agent::exec::what_arrived_on_one_pipe_is_not_reported_as_what_arrived_on_the_other`
`verified-by: bravebot_agent::exec::a_character_split_across_two_pipe_reads_is_handed_over_whole`
`verified-by: bravebot_agent::tools::a_job_output_wait_outside_the_bounds_is_refused_rather_than_shortened`
`verified-by: bravebot_agent::tools::job_output_offers_a_bounded_wait_rather_than_only_a_snapshot`
`verified-by: bravebot_agent::report::a_look_that_waited_is_described_with_both_the_window_and_the_warning`
`verified-by: bravebot_agent::report::one_second_is_described_in_the_singular`
`verified-by: bravebot_agent::turn::one_job_output_call_that_waits_is_handed_output_arriving_after_it_was_made`
`verified-by: bravebot_agent::turn::a_job_output_call_reports_the_code_a_finished_job_exited_with`

<a id="RUN-18"></a>
### RUN-18: the tool's own description routes a request to watch something to one of two techniques

`run`'s description must say that a request to watch something, or to be told when it changes, is
first a question about **what** is being watched, because a file and a program are watched by
different means. A file takes no command at all: `read_file` hands back a change token and the
technique is comparing one look's token with the next's, per
[read-file.md](read-file.md#READ-7). A program's own output is watched here, with `background: true`
and then `job_output` with `wait_seconds`, per [RUN-17](#RUN-17). Neither reaches past the turn by
itself: a background job is killed when the turn ends, and comparing a token needs a later look. So
the description must say to take the first look now and call `schedule_next` at the end of the turn,
which has the planner asked again after the wait with the person's line unchanged
([SCHED-6](schedule-next.md#SCHED-6), [LOOP-14](../loop.md#LOOP-14)); and that where no further look
has been scheduled, the answer says so.

**Why the file branch names no command.** The recipe this clause first shipped was `tail -f`, and it
was wrong twice over. It is not in the read-proven table, so every watch of a file cost an approval
the planner had no reason to expect, and it reports appends only, so a file truncated or replaced
under it looked untouched. A planner that reached for it anyway wrote its own poller instead, which
the command-line grammar refused. The token needs no command, no approval, and no program whose
output would come back quarantined.

**A turn already inside a loop is the third case, and gets a sentence of its own.** There the next
look is the next tick, so the description has such a turn report what this tick saw and leave the rest
to the next one, and arrange nothing. Without that sentence a tick reads the instruction above and
schedules a second look on top of the one its loop is already taking.

**The window, never a time of day.** The description must have the answer name the window it watched,
or the looks it compared, rather than date either, and must say why: the planner has no clock. It is told today's date and told not
to ask a program for the time, so an instruction to say when it looked is an invitation to invent an
hour, which is worse than the sample it was reporting.

**Why a clause about wording.** A tool's description is the only instruction the planner reliably
reads, so wording that changes behaviour is behaviour; [command-line.md](command-line.md) states that
generally. This one has its own case. Asked to say when a file changed, a session read
the file once, reported what it held, and left nothing watching; asked again, it read again and said
there was no change. Both techniques already existed, and the sentence carrying the bounded one was
about servers, so nothing joined a request to watch to either of them.

**And why the turn arranges the next look rather than the person.** This clause used to have the
description say that a loop was the person's to start and hand over the line to type, because at the
time it was. Both attempts at that sentence failed in the same place. The first named the command and
an interval, and a session that had taken a correct baseline told somebody to type `/loop 10s`, which
starts nothing: an interval with no request after it is not a line, so the command refused it. The
second handed over the whole line and was still an answer that ends in homework for whoever asked the
question. What the planner can do now is arrange the look itself, and a description that told it to ask
instead would be describing the worse of the two.

**Which is not the same as doing nothing.** Scheduling the next look is the end of the answer and not
the whole of it. Wording that offered a bounded branch and a loop branch, with no default for a request
that named no bound, sent an open-ended one to the branch that requires no work: asked to say when a
file changed, a session replied that a loop would be needed and made no tool call at all. That is a
worse answer than the single read this pair of clauses set out to correct, because the person was left
without even the file. Hence the first look happens in the turn that was asked.

`verified-by: bravebot_agent::tools::the_run_description_routes_a_watch_request_to_one_of_the_two_techniques`

<a id="RUN-19"></a>
### RUN-19: a prompt may record its answer past the session, and it stops only the asking

The run prompt offers a third answer, which outlives the session:

```
  y run it    a always this session    r remember it    n don't    ctrl-c stop the turn
```

`r` records this exact command line where the things that outlive a session are kept, under
`~/.bravebot`, beside the record of the session itself and keyed by the same directory
([state-directory.md](../state-directory.md), [sessions.md](../sessions.md)). Every session begun in
that directory honours what it holds, and the session that wrote it honours it from that moment. It
is consulted where a run prompt would otherwise be drawn rather than read once at the start, so a
line recorded a minute ago in another session is covered by this one, and a session with nobody to
put a prompt to consults nothing. A session that moves its working directory is answered by the
record for where it moved to and no longer by the one for where it was, because what `make check`
does depends on the tree it runs in and the person answered about one tree. The prompt shows what
would be recorded and where, because that is the whole of the grant and a person cannot endorse a
record they were not shown.

The row relabels `a` so that both lifetimes can be read off the screen, and it replaces
[RUN-7](#RUN-7)'s row wherever a run prompt is drawn, including the prompts that offer no `r`. What
`a` grants is unchanged. A bare `always` left on those prompts would be the one word this
relabelling exists to stop meaning two things, read by a person who has met the longer lifetime
elsewhere.

**What is recorded is the line, not text a pattern could be read out of.** The stage as it was
approved: the program's name, the binary that name resolved to, the argument list as it was given
with each argument its own field, every environment assignment the line carried with each name and
each value its own field, and where its output was sent. A later line is covered when every one of
those is the same and the name still resolves to the same binary, and in no other case. The name is
recorded as well as the binary because a name is what the person read, and a second name for the same
binary is a line they have not seen; [RUN-8](#RUN-8) is what keeps a name from carrying the coverage
on its own. Nothing in the record has a spelling that means "any text", so no key at this prompt can
reach a second line, which is [RUN-20](#RUN-20). A pipeline records every stage and is covered only
where every stage is, the same requirement [RUN-8](#RUN-8) makes of a vouch.

**The fields nobody would think to key on are the ones this turns on.** Sending the errors somewhere
else makes a different line, for the reason [RUN-6](#RUN-6) gives about a redirection being in
neither the program nor the arguments: a record made while two streams were merged would otherwise
cover the same program with them apart. Setting a variable makes a different line for a sharper
reason. An assignment decides what a program loads and reads before its own arguments are looked at,
which is why the hand-audited table refuses to prove a line carrying one
([command-line.md](command-line.md)), so a record that left it out would cover the line the person
read with anything at all put in front of it. This is still a narrower key than a vouch has. A vouch
is keyed on the resolved path and the exact arguments and nothing else ([RUN-8](#RUN-8)), so a
vouched entry covers the same program with its two streams merged; the session it was given in is
what bounds that, and a record that outlives the session has no such bound.

**The assignment is in the key and still cannot be recorded, and that is not a contradiction.** A
line carrying one is asked about before this record is reached ([RUN-8](#RUN-8)), so an entry holding
one would stop no later prompt, and `r` is not offered for such a line: a key that records something
answering nothing is a key that promises what the next session will not keep. The field stays in the
key so that the file cannot be made to say otherwise. It is read from the home directory, and an
entry arriving with an assignment left out of it, from a hand edit or from a version that keyed on
less, would cover the line a person answered about with anything at all put in front of it. Keying on
it makes that entry cover nothing rather than cover too much.

**A path in the record is the path's own bytes, and an entry holding a rendering covers nothing.** The
binary a name resolved to and the tree the line ran in are both paths, and a path read out of this file
next month is read by a build that need not be the one that wrote it. So it has the assignment's
problem in a sharper form: an entry arriving from a hand edit or from a version that recorded a
rendering names every binary whose path renders that way ([RUN-8](#RUN-8)), while the run spawns the
one the line named. Such an entry covers nothing, which puts the line back to a prompt exactly as a
record nobody had written would. Nothing this build writes is refused when read back, because a path
with no spelling as text is written as bytes, and so is one that genuinely holds the character a
rendering uses. The tree matters here for a second reason: the file's own name is lossy, so two trees
can share a record, and the path written into each entry is the only thing that tells them apart.

**Why a record rather than a rule in a settings file.** A rule is matched against a rendering of the
line: one string, the program's name and its arguments run together, in a language where a character
means "any text". Two different argument lists render to one string, and an argument containing that
character would grant the family this clause exists to refuse. A field per argument has neither
problem, and it can hold the binary the name resolved to, which a rule has no way to say.

**It grants the first of [RUN-7](#RUN-7)'s two things and never the second.** A covered line runs
unasked, side effects and all, and what it prints carries the label [RUN-4](#RUN-4) gives it, which is
untrusted and private. A person who wants what a command prints to be readable presses `a`, which is
a vouch and is RUN-7's.

**Why the key does not also vouch for the line in the session it was pressed in.** It could: the
person is present, so the assertion [RUN-7](#RUN-7)'s second half rests on is theirs to make, and the
entry would die with the session as every other one does. It is not done because one key would then
grant two things of different lifetimes, which is the reading the relabelled row exists to prevent,
and because it would lift the cost below for one session while leaving it in every later one. `a` is
the key that decides a label and this one is the key that decides a lifetime.

**The two records are separate, and a label is decided from the session's own.** What this one holds
answers whether to ask and nothing else, while the vouched list answers both questions. One list
serving both would make a covered line's output trusted on the strength of a keypress from a session
that has ended, and that is an assertion only a person who is present can make.

**Why the second could not last even though the person read this exact line.** RUN-7's second half is
an assertion about the thing that ran, made by somebody looking at it. A record read next month is
read on behalf of a person who is not looking, and the file at the recorded path may no longer be the
file they answered about. Recording the path narrows what may have changed to one binary replaced in
place, which is enough to keep the asking honest and not enough to carry an assertion about output.

**Which of the three things this could have weakened gives way: none of them.** The vouched list keeps
its key, its lifetime and its emptiness at the start of a session ([RUN-8](#RUN-8),
[RUN-9](#RUN-9)), because this record is not an entry in it and nothing here widens one. The trust map
is neither read nor written, so nothing about which paths are reachable moves. A durable grant is
usually refused on the ground that trust assumed from silence is not trust granted, and there is no
silence here: the person was asked, and the key they pressed is the one that says how long its answer
lasts. What lasts is the asking, which carries no trust, and that is why nothing had to give way to
let it. The clause that does change is elsewhere: how many standing grants there are, and which of
them a fresh session in the same directory reads, is [prompting.md](../prompting.md)'s, and this
record is a third one.

**It is read only where a prompt could be drawn, which is less than somebody reading one.** A
one-shot run and a session whose channel has closed read no record at all. What a record answers is a
prompt, and where no prompt can be put it would be saying instead which effects may happen with
nobody to put them to, which is what [cli.md](../cli.md) refuses an allow rule for and what the flag
that lifts that refusal is named and warned about for. A line in a file in the home directory must
not do that quietly, so a one-shot run puts a covered line where it puts every other one.

The boundary is the prompt and not the person, and the difference is where this is at its most
exposed. A tick of a loop, a round of a goal and a delegate's own step each draw their prompts to a
live session, so each reads the record and each runs a covered line without stopping, and a loop left
open overnight is a session where a prompt could have been drawn and nobody was there to read it
([loop.md](../loop.md) says what that costs). Somebody who presses `r` and then leaves a loop running
has granted more than somebody who presses it and stays. The line could not be drawn at the person
instead, because that would mean asking what a screen nobody is in front of would have said.

**A rule the person wrote in advance still decides.** A deny rule refuses a covered line outright and
an ask rule puts it back to them, because this record answers only where nothing they wrote already
does and a keypress must not overturn a standing instruction to be asked. So `r` is not offered where
such a rule matches the line, and one written afterwards takes the line back.

**Where `r` is not offered.** At a prompt for a run that releases private data, for the reason `a` is
not offered there ([RUN-6](#RUN-6)). Where the line names a file to write, and where it would run
anywhere but the workspace root, which takes in a line naming a directory
([CMDLINE-12](command-line.md#CMDLINE-12)) and a session with no root known: those are asked about
whatever is recorded, so the key would stop no prompt. Where the line writes an assignment in front of
a program, which is asked about whatever is recorded for the same reason and which `a` is not offered
for either ([RUN-8](#RUN-8)). `a` is not offered for a line that writes either, and
[RUN-6](#RUN-6) gives the reason: the entry would hold no destination, so the only line it could
cover is this one with the redirection gone. In a session that adds
nothing to `~/.bravebot`, which keeps a closed list of
what still reaches the filesystem and this is not on it ([incognito.md](../incognito.md)). In a
session answering every permission question without asking anybody, which draws no run prompt for a
key to reach: were one drawn, a record saying somebody chose to remember a line they were never shown
would be a standing permission nobody granted, which is the reason that mode adds nothing to the
vouched list either. Enter reaches no standing key, and declining or Ctrl-C records nothing, which is
what [prompting.md](../prompting.md) requires of every standing grant. Each of these refusals is made
twice, once where the prompt is drawn and again where a keypress is acted on, for the reason RUN-6
gives.

**A delegate's prompt may record a line, and nothing has to be handed back.** A delegate's prompts
reach the same person the session's own do ([permission-modes.md](../permission-modes.md)), and what
that person answers inside a delegate is already a decision about their own machine rather than the
delegate's ([delegation.md](../delegation.md)). The vouched list has to travel back out of a delegate
because it lives in the run, and this record does not, because it is a file every session begun in
that directory reads at the moment it would draw a prompt: a line recorded inside a delegate holds for
the turn that spawned it and for the next session, with no copy seeded and no difference collected.
The cost is that a delegate cannot be handed a narrower view of the record than the turn above it has,
and that a turn running ten of them has ten writers of one file, which is what an entry added rather
than a record rewritten is for. It is also the one thing two delegates share:
[DELEGATE-15](../delegation.md#DELEGATE-15) keeps each of them from seeing the other's conversation,
quarantine and vouched list, and this record is none of those, so a line one of them recorded stops
the asking in the next.

**No second question about the write.** Writing the record is a write, and every other write is put to
a person. This one is not, because the person pressed the key with the line and the place on the
screen, and asking again would collect a second answer for one decision. It is not a write any of
those gates governs either: it lands where what outlives a session lands, with the mode everything
there is written with ([state-directory.md](../state-directory.md)). What goes into it is the argument
list they read, trusted and public before it reaches any gate, so no byte a program printed reaches
the file. An entry is added rather than the record rewritten, so two sessions open in one directory
cannot lose each other's answers.

**It is read back with what put it there.** `/status` lists what is covered and says of each line
whether this session's own answer covered it or an earlier session's did, and where the list is
shortened it says how many of each it left out. The two lifetimes are the point: a person deciding
whether to press `r` again, or whether to delete something, cannot tell from a flat list which
answers they are still carrying from last week. The reading also says where the record is, because
deleting a line from it is the way back.

**The cost that matters most: a covered line is only as good as the tree it runs in.** `make check`
runs what the makefile in that tree has come to say, and `npm test` what its `package.json` has, so a
covered line does whatever the file it takes its work from now says. A vouch has the same property
and the session bounds it, where this record is bounded only by somebody deleting the line, in a
session that may be ticking a loop nobody is reading. The write
that changed such a file was itself put to a person, except in the mode that accepts edits without
asking, where it was not. Which programs take what they run from a file in the tree is what the
hand-audited table ([command-line.md](command-line.md)) knows and what this key does not consult, so
pressing `r` is a decision about the tree as much as about the command.

**Other known costs.** A binary replaced in place at the recorded path is covered without anybody
being asked again, which is a longer exposure than a session's and is what the answer lasting costs. A
covered line is never put to that person again, so it can no longer be vouched for and its output
stays quarantined: `read_output` still puts one result to them, which is [RUN-14](#RUN-14)'s advice,
and the way back is to delete the line. The record is keyed by a directory, so a person working in
two clones of one repository answers in each, and a session begun in a subdirectory is a different
directory from the one above it. That key is a directory's name with every character outside a small
set mapped to `-`, which is lossy, so two directories whose names reduce to the same one are answered
by one record; [sessions.md](../sessions.md) records the same cost of the session store, where it
takes a person resuming another directory's session to reach it, and here it takes only a session
begun in either. And a line whose arguments differ every time is not helped at all, which
[RUN-20](#RUN-20) is the answer to.

`verified-by: bravebot_core::remembered::the_line_that_was_recorded_is_covered`
`verified-by: bravebot_core::remembered::no_entry_reaches_a_second_argument_list`
`verified-by: bravebot_core::remembered::a_second_name_for_the_same_binary_is_not_the_line_that_was_read`
`verified-by: bravebot_core::remembered::an_answer_does_not_follow_a_name_onto_a_different_binary`
`verified-by: bravebot_core::remembered::an_environment_assignment_makes_a_different_line`
`verified-by: bravebot_core::remembered::sending_the_streams_somewhere_else_makes_a_different_line`
`verified-by: bravebot_core::remembered::a_pipeline_is_covered_only_where_every_stage_is`
`verified-by: bravebot_core::remembered::how_the_steps_are_joined_is_part_of_the_line`
`verified-by: bravebot_core::remembered::an_empty_record_covers_nothing`
`verified-by: bravebot_core::remembered::an_entry_says_which_session_answered_it`
`verified-by: bravebot_core::policy::a_line_remembered_past_the_session_is_not_asked_about`
`verified-by: bravebot_core::policy::output_of_a_line_remembered_past_the_session_is_still_untrusted_and_private`
`verified-by: bravebot_core::policy::a_line_remembered_past_the_session_vouches_for_nothing`
`verified-by: bravebot_core::policy::a_remembered_line_fed_private_input_is_asked_about_anyway`
`verified-by: bravebot_core::policy::a_remembered_line_that_writes_is_asked_about_anyway`
`verified-by: bravebot_core::policy::a_remembered_line_run_outside_the_root_is_asked_about_anyway`
`verified-by: bravebot_core::policy::a_remembered_line_carrying_an_environment_assignment_is_asked_about_anyway`
`verified-by: bravebot_core::policy::an_ask_rule_takes_back_a_line_remembered_past_the_session`
`verified-by: bravebot_core::policy::the_key_is_offered_for_a_line_nothing_refuses_it_for`
`verified-by: bravebot_core::policy::a_record_handed_over_again_replaces_what_it_held`
`verified-by: bravebot_agent::remembered::a_line_written_by_one_session_is_read_back_by_another`
`verified-by: bravebot_agent::remembered::every_field_of_a_line_survives_being_written_and_read`
`verified-by: bravebot_agent::remembered::a_second_answer_is_added_rather_than_replacing_the_first`
`verified-by: bravebot_agent::remembered::a_line_answered_in_one_directory_does_not_answer_in_another`
`verified-by: bravebot_agent::remembered::a_directory_sharing_a_key_with_another_is_not_answered_by_its_lines`
`verified-by: bravebot_agent::remembered::a_record_that_cannot_be_read_covers_nothing`
`verified-by: bravebot_agent::remembered::a_line_nothing_can_read_leaves_the_rest_of_the_record_answering`
`verified-by: bravebot_agent::turn::a_line_remembered_past_the_session_runs_without_asking`
`verified-by: bravebot_agent::turn::a_line_remembered_past_the_session_covers_no_other_line`
`verified-by: bravebot_agent::turn::a_turn_with_nobody_to_ask_reads_no_record`
`verified-by: bravebot_agent::turn::answering_with_a_key_the_prompt_did_not_offer_records_nothing`
`verified-by: bravebot_agent::permission_mode::bypassing_answers_every_permission_question`
`verified-by: bravebot_agent::incognito::no_remembered_line_is_written_down`
`verified-by: bravebot_agent::incognito::a_line_an_earlier_session_recorded_is_still_honoured`
`verified-by: bravebot_tui::confirm::the_run_keys_separate_this_session_from_every_session`
`verified-by: bravebot_tui::confirm::a_prompt_that_offers_no_record_binds_no_key_to_one`
`verified-by: bravebot_tui::confirm::enter_does_not_record_a_run_past_the_session`
`verified-by: bravebot_tui::confirm::refusing_a_run_records_nothing_past_the_session`
`verified-by: bravebot_tui::confirm::a_prompt_offering_to_remember_says_where_the_record_goes`
`verified-by: bravebot_tui::confirm::the_row_says_which_lifetime_the_always_key_grants`
`verified-by: bravebot_tui::confirm::a_prompt_with_no_record_to_offer_draws_no_key_for_one`
`verified-by: bravebot_tui::status::the_report_names_the_lines_remembered_past_a_session_and_who_answered_them`
`verified-by: bravebot_tui::status::a_shortened_list_of_remembered_lines_says_how_many_came_from_an_earlier_session`
`verified-by: bravebot_tui::status::a_directory_with_nothing_remembered_does_not_mention_the_record`
`verified-by: bravebot_tui::status::a_session_carrying_a_remembered_line_is_not_told_every_run_is_asked_about`
`verified-by: bravebot_agent::remembered::an_entry_this_build_does_not_fully_understand_covers_nothing`
`verified-by: bravebot_core::remembered::an_answer_does_not_follow_a_rendering_onto_a_different_binary`
`verified-by: bravebot_agent::remembered::a_binary_no_rendering_can_show_is_read_back_as_itself`
`verified-by: bravebot_agent::remembered::a_tree_no_rendering_can_show_is_answered_only_by_its_own_lines`
`verified-by: bravebot_agent::remembered::an_entry_whose_recorded_binary_is_a_rendering_covers_nothing`

<a id="RUN-20"></a>
### RUN-20: no answer at a prompt grants a family, because nothing here tells a value from a program

Neither `a` nor `r` covers more than one argument list, and no key derives a wider grant from the
lines a person has already answered.

**Why.** Bounding a family means knowing which argument positions carry a value and which name
something to run, and nothing available at a prompt can tell those apart. A wrapper puts what runs
into an argument: `npm run <script>`, `env NAME=1 <program>`, `ssh <host> <command>`, `sh -c <script>`,
`timeout 5 <program>`. A global option puts a sub-command into one: `git --no-pager <sub-command>`,
`cargo --quiet <sub-command>`. So no positional account holds. Freeing the last argument turns two
readings of a repository into a standing answer for `git --no-pager push`, and freeing any position but
the first turns two `npm run` lines into every script a `package.json` names, which is a file in the
tree rather than anything the person read. Two lines somebody has read do not say which position was
the value. A program-by-program account of which positions are values is a table checked by hand
against that program's full option list, which is the other road entirely
([command-line.md](command-line.md)) and is a proof rather than something a keypress mints.

**Why not a pattern the person edits at the prompt.** A box with `git commit -m *` already in it moves
the deciding back to this system, since whatever is filled in is what nearly every reader accepts, and
the thing filling it in is the thing that cannot tell a value from a program. An empty box is the
file's own work moved somewhere worse: a person composing a pattern there cannot see the rules they
already carry or the deny list theirs would sit under, and they are composing it in the middle of
answering a question about something else.

**Why the homework is the point here.** [RUN-18](#RUN-18) rejects an answer that hands somebody a line
to type where this system could do the thing itself, and that reasoning does not reach this case. What
is handed over is not a chore but an authority: deciding which argument carries a value is a judgment
about a program, and nothing here can make it. Where the line repeats, nothing is handed over at all
and [RUN-19](#RUN-19)'s key is the whole of the answer. The lines that vary are the only ones left,
and they are the person's because only the person can decide them.

**What the prompt says instead.** Where the line being asked about is one whose arguments will differ
next time, the prompt says that a pattern is written in a settings file rather than answered at a
prompt, names the file, and says what a pattern costs: it covers lines nobody has read, it stops the
asking, and it makes nothing readable. A file they edited is also one they can read back and delete.
Saying nothing
would be the worse answer, for the reason [RUN-14](#RUN-14) says what would lift a quarantine rather
than leaving it to be found: somebody answering the same shape of prompt all day learns nothing from
the prompt about the durable form. No pattern is put on the screen, for the reason a box with one in
it is refused above: naming the file is the fact they cannot get from anywhere else, and composing
the rule is the judgment only they can make.

**How a prompt knows which line that is.** The person has already read a run prompt for the same
binary, in this session, under a different argument list. Two argument lists for one program is the
variation itself rather than a reading of the argv, and nothing in it says which position moved, so
this establishes what the clause needs without making the judgment the clause refuses. It is read
off the run prompts this session has drawn, which is a list that grants nothing: membership stops no
prompt, raises no label, vouches for nothing and reaches no file. It is written nowhere, not even
into the session record that carries the vouched list across a `--resume` ([RUN-9](#RUN-9)), because
what it holds is questions somebody read rather than anything they are carrying. So the first prompt
of a session says nothing about patterns, and so does the first prompt of a resumed one. Neither does
any prompt on a machine that names no home directory, since there is no file to name and advice that
cannot say which file is a chore handed over without the one fact it needs.

The whole line is compared at once, and the argument lists the line itself holds are not what it
differs from. `grep TODO src | grep -v test` names one binary under two argument lists, so a
step-by-step reading would have that line, asked about a second time, varying from itself.

**Only where a rule would decide the line.** The advice says that editing a file ends the asking, so
it is given only where that is true. A line releasing private input, naming a file to write, running
anywhere but the workspace root, or writing an assignment in front of a program is put to a person
before any rule is read ([RUN-6](#RUN-6), [RUN-8](#RUN-8)), so no pattern reaches one and saying
otherwise would have somebody change the wrong thing about the line. These are the same four
refusals [RUN-19](#RUN-19)'s key is withheld for, and for the same reason: each is made before the
thing being offered is consulted. Where a rule already matches the line, nothing is said either,
since the person has found the file and what their rule says is what happens.

**Why not on every prompt.** [RUN-19](#RUN-19)'s key is the whole of the answer for a line that
repeats, so the advice there would send somebody to edit a file where a keypress would do. It is
drawn beside that key rather than instead of it, because a line can repeat and vary in one session
and both answers are then true of it.

**A known cost.** A line whose arguments change every time is asked about every time, in this session
and in the next. A commit message and a new branch name are the two that do this in ordinary work.
The answer for them is a pattern the person writes or a prompt each time, and this clause chooses the
prompt.

**A second known cost.** Two different jobs for one program read as one job whose arguments moved.
`git log` followed by `git push` draws the advice, because telling those apart means deciding which
argument names something to run, which is what this clause refuses to decide for a grant and cannot
decide here either. What it costs is a sentence of advice where a person did not need one, against
the alternative of withholding it from the case it exists for.

`verified-by: bravebot_core::programs::a_second_argument_list_for_one_binary_is_a_line_whose_arguments_vary`
`verified-by: bravebot_core::programs::a_line_asked_about_twice_is_not_a_line_whose_arguments_vary`
`verified-by: bravebot_core::programs::a_line_nothing_has_been_asked_about_has_no_arguments_that_have_varied`
`verified-by: bravebot_core::programs::a_second_binary_is_not_the_first_ones_arguments_varying`
`verified-by: bravebot_core::programs::asking_about_a_line_vouches_for_nothing`
`verified-by: bravebot_core::policy::a_binary_asked_about_under_two_argument_lists_is_one_whose_arguments_have_varied`
`verified-by: bravebot_core::policy::a_line_asked_about_again_unchanged_has_not_varied`
`verified-by: bravebot_core::policy::a_line_asked_about_is_not_thereby_vouched_for_or_remembered`
`verified-by: bravebot_core::policy::a_line_the_rules_are_never_read_for_is_one_no_pattern_would_answer`
`verified-by: bravebot_core::programs::a_line_naming_one_binary_twice_does_not_vary_from_itself`
`verified-by: bravebot_agent::turn::a_line_no_rule_is_ever_read_for_is_advised_no_pattern`
`verified-by: bravebot_core::policy::vouching_for_one_command_does_not_cover_another_of_the_same_program`
`verified-by: bravebot_core::remembered::no_entry_reaches_a_second_argument_list`
`verified-by: bravebot_agent::turn::a_binary_asked_about_under_two_argument_lists_is_advised_to_a_settings_file`
`verified-by: bravebot_agent::turn::a_line_asked_about_again_unchanged_is_advised_no_pattern`
`verified-by: bravebot_tui::confirm::a_prompt_for_a_line_whose_arguments_vary_names_the_settings_file`
`verified-by: bravebot_tui::confirm::a_prompt_for_a_line_whose_arguments_vary_says_what_a_pattern_costs`
`verified-by: bravebot_tui::confirm::advising_a_pattern_offers_no_key_that_grants_one`
`verified-by: bravebot_tui::confirm::a_prompt_for_a_line_nothing_has_varied_says_nothing_about_a_pattern`

<a id="RUN-21"></a>
### RUN-21: the cap on what output may spend of the conversation is configurable

`run.maxOutput`, in the settings files, names how many bytes of what a program printed may enter the
conversation. It may be raised as well as lowered, and a key nobody set leaves the built-in cap in
force. One key covers a foreground run, a `job_output` page, and the account a finished background
job gives of itself, because those are the same bytes reaching the same context by three routes. It
does not bound `read_output`, which hands over the whole of the slot it was endorsed for
([OUTPUT-1](read-output.md#OUTPUT-1)). It does bound the same release made in a run's own result
([RUN-22](#RUN-22)), which is asked for before the size is known.

A cap of zero is absence rather than a program permitted to say nothing, as is any value that is not
a whole count. Absence leaves the built-in cap in force, which is what a layer setting the key gets
rather than the number a weaker layer named.

**Why it may be configured at all.** The cap bounds a budget and not an authority. A gate decides
what may run and what may be read; this decides how much of a result is worth the room it takes, and
the room belongs to whoever is paying for the conversation. A build log worth 60 KB to the person
reading the turn is worth that whatever this program's default says, and a raised cap takes nothing
away from a gate: output nobody vouched for is quarantined whole either way, and what the cap sets is
how much of what the planner may read reaches it.

**It bounds the conversation and never the run.** What was printed is kept whole beside the sample
whatever the cap is, so the middle is still there to hand to a processor or write to a file, and a
raised cap does not mean a command is run twice to see it.

**The figure is in bytes.** Cutting on characters would make one number mean a different amount of
context per language, and a cap is spent in tokens rather than in glyphs. The cut still lands on a
character boundary, because a multi-byte character straddling it would end the turn on output nobody
chose.

**Where it is resolved.** In the caller that read the settings, as [SEARCH-9](search.md#SEARCH-9)'s
caps are and for the same reason: every test in the tree builds a turn, and one that read a settings
file would answer differently on a machine whose owner had configured it. A delegate runs under the
figure the turn that spawned it holds, since the budget is a person's answer about what output is
worth and does not stop being theirs because the work moved ([DELEGATE-1](../delegation.md)).

`verified-by: bravebot_config::settings::a_settings_file_names_what_a_commands_output_may_spend`
`verified-by: bravebot_config::settings::a_cap_of_zero_or_of_nonsense_leaves_the_built_in_one`
`verified-by: bravebot_config::settings::the_nearest_layer_that_named_an_output_cap_wins`
`verified-by: bravebot_agent::tools::output_is_cut_to_the_cap_it_was_given`
`verified-by: bravebot_agent::turn::a_configured_output_cap_is_what_a_run_is_cut_to`
`verified-by: bravebot_agent::turn::a_delegate_runs_under_the_output_cap_of_the_turn_that_spawned_it`

<a id="RUN-22"></a>
### RUN-22: a run may ask to read what it printed, and gets it in the same result where nobody would be asked

`read: true` asks for what the line printed in the result that ran it. Where permissions are
bypassed with no screening asked for ([MODE-4](../permission-modes.md#MODE-4)), in a turn offered
`read_output`, that is what the planner gets. The output is quarantined as [RUN-4](#RUN-4) has it
and then released by `read_output`'s own path, so the value is `(T,priv)`, the trail credits the
mode in the entry [OUTPUT-1](read-output.md#OUTPUT-1) writes, and nobody is shown anything. The slot
keeps its name, and the result gives the name beside the text, so the same bytes can still be fed to
a line or written by reference.

[RUN-21](#RUN-21)'s cap bounds the release, where it does not bound `read_output`. A `read_output`
call is made by a planner that has seen the reference and the size it states, and `read` is asked
before there is a size to see. Output longer than the cap stays behind its reference, and the
result says it was too long for a run's result rather than pointing at `read` again. It says with
that where the rest is: `read_output` is not held to the cap, and a filter given the reference as
`stdin_ref` reads part of it without the line being run again. The size is the slot's, as its
reference states it, so no byte the program printed is read to decide.

Everywhere else `read` changes nothing. Where a person or a check answers `read_output`, the output
is quarantined, nobody is asked and no check is made, since a prompt or a check made here would be
spent on output the planner has not called `read_output` for. Where the turn was not offered
`read_output`, as a definition may leave it ([ADDRESS-7](../addressing-a-definition.md#ADDRESS-7),
[DELEGATE-4](../delegation.md#DELEGATE-4)), `read` would widen what it was confined to, so it is not
a release there either. A `read` that is absent or null is a call without it, and one that is
neither `true` nor `false` is refused before the line runs, as a mistyped `stdin_ref` is: a planner
that believed it had asked would be handed a reference, and would ask again by running the line
again. `read` beside `background: true` is refused, as `stdin_ref` is ([RUN-3](#RUN-3)): the result
that starts a job holds nothing the job printed.

**Why.** Under unscreened bypass `read_output` has one answer, so a round spent calling it buys
nothing but the round: a model call, and the whole conversation sent again. Doing it on request
rather than for every run keeps output the planner does not want out of its context, where a build
or test log is often the largest thing a turn produces and is often read only for how it ended,
which [RUN-13](#RUN-13) already says.

**Why the result says where the rest is.** Told only that the output was too long for one result, a
planner took `read_output`'s result to be held to the same size, and ran a four-minute `make check`
a second time through `grep` to learn which tests had failed. The whole log was in the slot the
first run filled. Both routes it names release by the paths above and no other, so saying them
changes what the planner knows to ask for and nothing it may be given.

**This changes no label.** The value and the trail entry are the ones `read_output` makes, a round
early. What decides whether the release happens is the mode, the tools the turn was offered, the
slot's size and the planner's own argument. The mode is the person's answer given before the session
started, and a planner offered `read_output` may call it on any reference it holds, so asking with
`read` gives it nothing it could not already have. No byte the program printed is read to decide it.

`verified-by: bravebot_agent::tools::run_takes_one_command_line_and_nothing_else`
`verified-by: bravebot_agent::turn::an_unscreened_unattended_run_that_asks_to_read_is_handed_its_output_in_the_same_result`
`verified-by: bravebot_agent::turn::an_unscreened_unattended_run_that_does_not_ask_to_read_hands_back_a_reference_and_says_how_to`
`verified-by: bravebot_agent::turn::asking_to_read_a_runs_output_changes_nothing_where_anybody_still_answers_for_it`
`verified-by: bravebot_agent::turn::a_background_line_cannot_ask_to_read_what_it_printed`
`verified-by: bravebot_agent::turn::asking_to_read_does_not_hand_output_to_a_definition_left_without_read_output`
`verified-by: bravebot_agent::turn::asking_to_read_more_than_one_result_may_hold_hands_back_the_reference_and_says_why`
`verified-by: bravebot_agent::turn::output_too_long_for_its_result_is_read_in_part_by_a_filter_fed_its_reference`
`verified-by: bravebot_agent::turn::a_read_that_was_refused_is_not_asked_for_again`
`verified-by: bravebot_agent::turn::a_read_that_is_not_true_or_false_is_refused_before_the_line_runs`

## Open questions

- Whether output can ever be trusted by proof rather than by assertion is issue #3, and it may not
  be resolved by weakening RUN-4.
- A separate proof path reaches RUN-4's trusted label by the other road, proving from the program
  and its arguments that a stage can read nothing the label does not account for. It is a proof about a program where
  RUN-7 is a person taking responsibility for one, and the two must not be merged. It remains
  unwired.
