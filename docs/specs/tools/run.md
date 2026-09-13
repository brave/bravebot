---
id: RUN
title: run
status: normative
governs:
  - crates/agent/src/exec.rs
  - crates/agent/src/scrub.rs
  - crates/core/src/command.rs
  - crates/core/src/programs.rs
  - crates/core/src/policy.rs
  - crates/tui/src/confirm.rs
guards:
  - symbol: Policy::read_output
  - symbol: Policy::remember_command
  - symbol: TrustedPrograms::trust
---

## Scope

`run`, its labels, and the two ways a person can change them. Shell mode is [shell-mode.md](../shell-mode.md) and is not an
instance of this: it is not a tool and the planner cannot reach it.

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
`verified-by: bravebot_core::policy::an_empty_pipeline_is_refused`

<a id="RUN-2"></a>
### RUN-2: the plan is routing and must be endorsed by a person

Programs, arguments, the directory a plan runs in, and the files it writes must be `(T,pub)`.
Untrusted text never becomes one. The endorsement is bound to that exact plan, so it cannot be reused for a different one: not
for the same steps joined differently, not for the same steps writing somewhere else, and not for
the same steps in another directory.

`verified-by: bravebot_core::policy::a_run_without_an_endorsement_is_refused`
`verified-by: bravebot_core::policy::a_plan_without_an_endorsement_is_refused`
`verified-by: bravebot_core::policy::an_endorsement_does_not_authorise_a_plan_that_writes_elsewhere`
`verified-by: bravebot_agent::exec::a_stage_runs_the_binary_it_was_resolved_to`
`verified-by: bravebot_agent::exec::a_pipeline_with_missing_resolutions_does_not_run`

<a id="RUN-3"></a>
### RUN-3: stdin is content and may be untrusted

The planner names a quarantined reference and the policy layer supplies the bytes, so `sed` and `awk`
work on a file nobody vouched for without the planner or the driver ever reading it. A stage that
reads stdin and was given none receives nothing, never the terminal.

**Why.** This is the point of the split: both trusted and untrusted data reach real tools, and
only the routing part has to be trustworthy.

`verified-by: bravebot_agent::exec::a_stage_that_reads_stdin_is_given_nothing_rather_than_the_terminal`

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

`verified-by: bravebot_core::command::a_file_redirected_into_a_program_is_private_input`
`verified-by: bravebot_core::command::a_redirection_on_a_later_step_is_private_input`
`verified-by: bravebot_core::command::a_plan_that_feeds_a_program_nothing_releases_nothing`
`verified-by: bravebot_core::policy::output_nobody_vouched_for_is_untrusted_and_private`
`verified-by: bravebot_core::policy::output_of_a_vouched_command_is_trusted`
`verified-by: bravebot_core::policy::output_of_a_vouched_command_is_still_private`
`verified-by: bravebot_core::policy::one_unvouched_stage_makes_the_whole_output_untrusted`

<a id="RUN-5"></a>
### RUN-5: every run asks, unless every stage was vouched for or proven

There is no *declared* read-only category. `foo --bar` might write to disk and nothing here can
tell, and a stage declaring itself harmless only helps if the declaration is honest. Two things may
answer the question, and nothing else: a person having answered it before, in this session, for this
exact command, and the audited table in [command-line.md](command-line.md) establishing that these
exact arguments write nothing and read only paths the user vouched for. Never a property of the
argv, never a declaration by a stage, never anything derived from what a program printed.

**Why.** An unprompted write is worse than an unwanted prompt, so nothing that could be wrong about
a write may answer the question. An entry in the table is a claim checked by hand against one
program's full option list, which is why it may, and it is narrow for the same reason: anything it
does not fully recognise asks.

`verified-by: bravebot_core::policy::a_command_nobody_vouched_for_is_put_to_a_person`
`verified-by: bravebot_core::policy::a_vouched_command_is_not_asked_about_again`
`verified-by: bravebot_core::policy::one_unvouched_stage_puts_the_whole_pipeline_to_a_person`
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

`verified-by: bravebot_core::policy::private_input_asks_even_for_a_vouched_command`
`verified-by: bravebot_core::policy::private_input_asks_even_for_a_vouched_line`
`verified-by: bravebot_agent::cmdline::an_input_redirection_is_private_input`
`verified-by: bravebot_agent::turn::a_line_that_reads_a_file_is_not_remembered_however_it_is_answered`
`verified-by: bravebot_tui::confirm::a_run_reading_a_file_offers_no_standing_permission`
`verified-by: bravebot_tui::confirm::a_run_that_releases_private_data_offers_no_standing_permission`
`verified-by: bravebot_tui::confirm::pressing_always_at_a_private_input_prompt_grants_nothing`

<a id="RUN-7"></a>
### RUN-7: vouching grants two things together, and the prompt asks for both

```
  y run it    a always    n don't    ctrl-c stop the turn
```

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
### RUN-8: an entry is keyed by resolved path and exact arguments

`git log` says nothing about `git push`, and nothing about `git log --all`. `$PATH` and aliases
decide what a name means, so an assertion must not follow a name onto a different binary. Never
widen an entry to a program alone. In a pipeline **every** stage must be vouched for or the whole
output is untrusted, since an unvouched stage in the middle is a transformation nobody answered
for and its output is what the next stage read.

An entry also says nothing about **where** the command runs, because nothing in it records a
directory. So it grants neither of RUN-7's two things outside the workspace root: a line naming a
directory ([CMDLINE-12](command-line.md#CMDLINE-12)) is asked about however often it was vouched
for, and what it prints is `(U,priv)`. `git log` pointed at a vendored dependency prints commit
messages from a repository the person never answered a question about. Widening the key to include
the directory would grant the shortcut there, and it is not done: it would put a tree into an entry
the session record and `/status` (RUN-9) describe as a program and its arguments.

`verified-by: bravebot_core::policy::vouching_for_one_command_does_not_cover_another_of_the_same_program`
`verified-by: bravebot_core::policy::vouching_does_not_follow_a_name_onto_a_different_binary`
`verified-by: bravebot_core::policy::a_vouched_line_is_asked_about_when_it_runs_outside_the_root`
`verified-by: bravebot_core::policy::output_of_a_vouched_line_run_outside_the_root_is_untrusted`
`verified-by: bravebot_core::policy::a_vouched_line_is_asked_about_when_no_root_is_known`
`verified-by: bravebot_agent::turn::a_vouched_line_is_asked_about_again_when_a_directory_is_named`

<a id="RUN-9"></a>
### RUN-9: the vouched list belongs to the session

Empty at the start of every session, written into the session record, restored by `--resume`,
never inherited by a fresh session in the same directory. `/status` lists what was granted.

**Why.** The same reason the trust map belongs to a session. It is the one permission whose whole effect is that
prompts stop, so it has to be readable back.

`verified-by: bravebot_core::policy::a_fresh_policy_vouches_for_no_command`

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

`verified-by: none`

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
authenticates to its backend with. Every stage, not only the first. Removed rather than emptied, so a
program that distinguishes an unset variable from a blank one sees what a machine that never held the
credential sees.

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
of the user's own. What closes here is the narrow part of the gap, the credentials a person could
not have been shown at the prompt and had no way to withhold. The rest of what they hand over is
still handed over. Nothing is established about what the program then does, and the label on the
output is unaffected.

A line a person typed themselves is not this, and keeps the whole environment: it is meant to behave
as their own terminal does, and nothing else about it is gated either.

`verified-by: bravebot_agent::exec::this_agents_own_credentials_do_not_reach_a_program_it_runs`
`verified-by: bravebot_agent::exec::no_stage_of_a_pipeline_sees_this_agents_credentials`
`verified-by: bravebot_agent::exec::the_users_own_environment_still_reaches_a_program`
`verified-by: bravebot_agent::exec::the_plumbing_a_program_needs_is_still_inherited`
`verified-by: bravebot_agent::exec::the_filtering_can_be_switched_off_by_its_documented_spelling_only`
`verified-by: bravebot_agent::scrub::this_agents_credentials_are_withheld_without_being_configured`
`verified-by: bravebot_agent::scrub::nothing_of_the_users_own_is_withheld_by_guesswork`
`verified-by: bravebot_agent::scrub::a_name_from_the_settings_file_is_withheld_as_well`

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
`read_output` or about vouching for a command nobody ran.

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
no reader for. Both are refused rather than half-honoured.

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

`run`'s description must say that a request to watch something, or to be told when it changes, is a
decision about how long for, and must name both branches. Bounded and inside the turn is
`background: true` and then `job_output` with `wait_seconds`, per [RUN-17](#RUN-17). Past the end of
the turn is a loop, and the description must say that the planner cannot start one, because a
background job is killed when the turn ends: a loop is a person's to start, by typing `/loop`, which
is [loop.md](../loop.md). It must also say that a single read is neither, and that where nothing is
watching, the answer says so.

**A turn already inside a loop is the third case, and gets a sentence of its own.** There the next
look is the next tick, so the description has such a turn report what this tick saw and leave the rest
to the next one. Without that sentence the loop branch reads as an instruction to ask for a loop, which
in a tick is asking for something the person has already given.

**The window, never a time of day.** The description must have the answer name the window it watched
rather than date it, and must say why: the planner has no clock. It is told today's date and told not
to ask a program for the time, so an instruction to say when it looked is an invitation to invent an
hour, which is worse than the sample it was reporting.

**Why a clause about wording.** A tool's description is the only instruction the planner reliably
reads, so wording that changes behaviour is behaviour; [command-line.md](command-line.md) states that
generally. This one has its own case. Asked to say when a file changed, a session read
the file once, reported what it held, and left nothing watching; asked again, it read again and said
there was no change. Both techniques already existed, and the sentence carrying the bounded one was
about servers, so nothing joined a request to watch to either of them.

**And why the loop branch says who starts it.** A description that told the planner to use a loop
would be describing something it has no way to do, and the failure it invites is worse than doing
nothing: a turn that reports a watch it never started. What the planner can do is say that, and say
what the person would type.

`verified-by: bravebot_agent::tools::the_run_description_routes_a_watch_request_to_one_of_the_two_techniques`

## Open questions

- Whether to confine children is issue #4. Whether output can ever be trusted by proof rather than
  by assertion is issue #3. Neither may be resolved by weakening RUN-4.
- A separate proof path reaches RUN-4's trusted label by the other road, proving from the program
  and its arguments that a stage can read nothing the label does not account for. It is a proof about a program where
  RUN-7 is a person taking responsibility for one, and the two must not be merged. It remains
  unwired.
