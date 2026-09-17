---
id: CLI
title: The command line
status: normative
governs:
  - crates/cli/src/main.rs
  - crates/cli/src/exit.rs
  - crates/cli/src/json.rs
---

## Scope

Running bravebot without the interactive interface: a one-shot task, piped input, `doctor`, and
what goes where on the way out. The interactive session is
[terminal-input.md](terminal-input.md) and [terminal-transcript.md](terminal-transcript.md).

A one-shot run has nobody to ask, and most of what makes it different follows from that.

## Clauses

<a id="CLI-1"></a>
### CLI-1: where nobody can be asked, nothing is approved

Effects are refused rather than applied unseen, and the planner's own questions are declined
rather than answered on the user's behalf. A rule written in advance that refuses, or that forces
an ask, holds here as it does in a session; one that allows does not, since what it answers is a
prompt and there is nobody to prompt.

`--dangerously-skip-permissions` is the one way a run nobody is watching may write, and it is the
person's own instruction rather than a default: what it selects, and what it costs, is
[permission-modes.md](permission-modes.md). The planner's questions are declined in that mode too,
because they are not permissions.

**Why.** The alternative to a person is not a default, it is a guess made in their name. The
planner is told a reply came from a person, so inventing one would be worse than not asking. A flag
somebody typed is not a guess, which is what makes it the only thing that may lift the first half of
this and nothing that may lift the second.

An allow rule is not a guess either, and that is not what disqualifies it. It says which prompts to
stop raising, which is a decision about a session somebody is sitting in front of; read here it
would say which effects may happen unwatched, and one line in a file in the home directory would
do what the flag is named and warned about for. A record of command lines somebody asked to be
remembered past a session ([tools/run.md](tools/run.md)) is not read here either, and for this same
reason.

`verified-by: bravebot_agent::turn::an_unattended_run_declines_every_question_in_the_series`
`verified-by: bravebot_agent::turn::a_refused_write_does_not_happen`
`verified-by: bravebot_agent::turn::a_turn_with_nobody_to_ask_reads_no_record`
`verified-by: bravebot_cli::main::permissions_are_enforced_unless_the_flag_is_given`
`verified-by: bravebot_cli::main::an_allow_rule_decides_nothing_for_a_run_nobody_is_watching`
`verified-by: bravebot_cli::main::the_flag_is_what_lets_an_allow_rule_decide_again`

<a id="CLI-2"></a>
### CLI-2: unprompted, stdin is read only when it is not a terminal

A terminal's stdin is left alone, so an interactive invocation does not sit waiting for input
nobody is sending. Piped bytes are read when there are any.

A question this run asked is the exception, and it is not the case this clause is about: the bytes
are read because somebody was prompted for them a moment earlier, so they are input that is being
sent. The one such question is the plan in CLI-8, and it is asked only where stdin is a terminal, so
a pipe is never read for an answer.

`verified-by: bravebot_cli::main::a_terminal_stdin_is_not_read`
`verified-by: bravebot_cli::main::piped_bytes_are_read_when_stdin_is_not_a_terminal`

<a id="CLI-3"></a>
### CLI-3: piped input is untrusted and private, always

Nothing vouched for what a pipe carries: `gh pr diff` and `cat build-error.txt` both arrive the
same way and neither passed through the trust map. So it is quarantined and the planner is given a
reference, never the bytes.

**Why.** A pipe has no path, so there is nothing for the trust map to have an opinion about. The
pessimistic label is the only one that holds without knowing what fed it.

`verified-by: bravebot_core::policy::piped_input_is_labelled_untrusted_and_private`
`verified-by: bravebot_core::policy::piped_input_is_quarantined_when_presented`
`verified-by: bravebot_agent::turn::piped_input_is_never_shown_to_the_planner`

<a id="CLI-4"></a>
### CLI-4: input over the cap is refused, and says what to do instead

Rather than truncated, since a silently shortened input is one the planner would answer about
having seen part of.

`verified-by: bravebot_cli::main::input_over_the_cap_is_refused`
`verified-by: bravebot_cli::main::a_refused_pipe_says_what_to_do_instead`

<a id="CLI-5"></a>
### CLI-5: stdout carries the reply and nothing else

Progress, errors and the audit trail go to stderr, so a one-shot run is pipeable. `--trace` puts
the trail on stderr beside it: which gate checked what, the label every value carried, and what
was released. The one thing that may take the reply's place on that stream is the result object in
CLI-12, and it is still the only thing on it.

**Why.** A progress line mixed into stdout would corrupt whatever the user piped the reply into.

`verified-by: bravebot_cli::main::stdout_carries_the_reply_and_nothing_else`
`verified-by: bravebot_cli::main::an_untraced_run_writes_no_trail`
`verified-by: bravebot_cli::main::the_trail_renders_a_line_for_every_event`

<a id="CLI-6"></a>
### CLI-6: a failure exits with a status that says which failure, and says an identifier

A configuration error, a refused argument, and a turn that could not run all fail rather than
exiting successfully with an explanation on stdout, and each of them has a status of its own:

| Status | Identifier | The run |
|---|---|---|
| 0 | | did what it was asked |
| 1 | `BB1001` | failed for a reason none of the others name |
| 2 | `BB1002` | refused an argument, so nothing ran |
| 3 | `BB1003` | cannot use the configuration, so nothing ran |
| 4 | `BB1004` | had an effect refused by a gate |
| 5 | `BB1005` | never reached the backend |

A status is never renumbered and never given a second meaning. A failure kind nothing here names
is 1, and one worth telling apart takes the next number.

The identifier is printed in front of the message on stderr, never instead of it, and is the same
whatever language the message is in.

**Why.** A caller cannot act on a run it cannot classify. "The endpoint was not there, try again",
"the configuration is wrong, fail the build" and "a gate refused the write, this needs a person"
are three different things to do about a failed run, and with one status for all of them a script
can do none of them. Which of these a failure is, is something the program knows at the moment it
exits, so the alternative to saying it is throwing it away.

Only the transport's own failures are a backend that was not there. A non-success status is the
service answering, and a caller that read a refused credential as a connection to try again would
retry it until it gave up.

The identifier exists because the message does not survive being passed on. A sentence in the
reader's own language is the right thing to print and the wrong thing to search for: pasted into a
bug report it reaches somebody who cannot grep it, and the status was never part of the text at
all. It is derived from the status rather than allocated separately, because two numbering schemes
over one set of failures is one of them going out of date.

`verified-by: bravebot_cli::running::a_configuration_error_exits_non_zero`
`verified-by: bravebot_cli::running::a_refused_argument_exits_non_zero`
`verified-by: bravebot_cli::running::a_turn_that_could_not_run_exits_non_zero`
`verified-by: bravebot_cli::running::each_kind_of_failure_has_a_status_of_its_own`
`verified-by: bravebot_cli::running::a_failure_says_a_stable_identifier_whatever_language_it_explains_itself_in`
`verified-by: bravebot_cli::exit::every_ending_has_a_status_of_its_own`
`verified-by: bravebot_cli::exit::a_failure_is_identified_and_a_success_is_not`
`verified-by: bravebot_cli::exit::a_failure_says_its_identifier_in_front_of_the_message`
`verified-by: bravebot_cli::exit::a_manifest_run_is_classified_by_what_stopped_it`
`verified-by: bravebot_agent::backend::a_request_that_never_left_is_told_apart_from_one_that_was_answered`
`verified-by: bravebot_cli::main::a_turn_something_was_refused_in_does_not_succeed`

<a id="CLI-7"></a>
### CLI-7: `doctor` reports configuration and confinement without changing anything

It prints every backend this build can reach and what identifies it, which names the settings set,
which settings files are in force and which of them won a name more than one set, which names a
machine-level file pinned and where that file is, how to configure a service where nothing
configured will serve a turn, the model in force
and whether it was chosen or defaulted, where the state directory is or that there is none, what a
TLS handshake is validated against and what a request is routed through, the
confinement available on this platform, and the state of any imported subscription. The signing key
is named as never transmitted, and a value from a settings file is never printed: where a credential
decides whether a backend works, what is reported is that one was found. A configuration error makes
it fail rather than pass with a warning.

The state directory is reported with the variable that named it, and on a platform where the files
under it cannot be restricted to one account the report says which of them carry the permissions of
the profile directory instead. Where there is no state directory, the report says which variables were
looked at, points the remedy at those same variables, names what is not kept without one, and says
that a checkout's own settings, skills and instructions are read regardless. It is reported rather than failed on, and sits outside the
configuration section, which a configuration error stops early.

In a Bravebot source checkout (including its subdirectories), it also reports whether root
`AGENTS.md` resolves to `agents/AGENTS.md` and whether `direnv` is executable on PATH. These
are development advice and do not change the exit status. Ordinary workspaces show neither
check. Discovery stops at the nearest Git checkout boundary. The source checkout is recognised
by its root and CLI Cargo manifests, `agents/setup.py`, `agents/AGENTS.md`, and
`docs/development/agent-configuration.md`. Missing, broken, or wrongly targeted links recommend
`python3 agents/setup.py link` at the checkout root. Real files and directories are conflicts to
resolve first; on Windows a matching copy is healthy and a stale copy recommends setup again. No
path is changed. Missing direnv points to https://direnv.net/ and `brew install direnv`; shell
hooks and `.envrc` approval are outside this check.

`verified-by: bravebot_cli::main::doctor_development_checks_only_apply_to_the_source_tree`
`verified-by: bravebot_cli::main::doctor_reports_agent_discovery_conflicts_without_changing_them`
`verified-by: bravebot_cli::main::doctor_accepts_current_windows_copies_and_reports_stale_ones`
`verified-by: bravebot_cli::main::doctor_checks_resolved_agent_link_targets`
`verified-by: bravebot_cli::main::doctor_finds_direnv_only_when_path_contains_an_executable`

**Why.** It exists to answer "what will this actually use", so reporting a default when a choice
is in force would explain the wrong thing, and naming one backend where two are reachable would
explain only the half somebody happened to ask about. Naming the files is the same argument: settings
resolve across three of them, so a value somebody did not expect has three places it could have come
from and the path is the whole of what narrows it to one. Values are withheld because a settings file
holds credentials on some machines, and a diagnostic that prints one is a diagnostic people paste
into issues. Whether one was found still has to be said, because a backend nothing can authenticate
is the case this is most often run to explain.

A pinned name is named for a stronger version of the same reason. A value a person cannot change
from anywhere they can write has to be explained somewhere, or the report shows a host they did not
choose beside a variable of theirs that is doing nothing, and nothing on the machine says why. The
file is named beside the names because the remedy belongs to whoever can write it rather than to the
reader. A file that is there is named even where nothing in it was pinned, whether because it holds
no pinnable name or because nothing could read it, since either is otherwise indistinguishable from
a file this program never found. It is named on a configuration error too, that being the one case
where nothing the reader can write will fix what the report is complaining about.

The state directory is the same argument one step further out. What outlives a session is kept in it,
and [STATE-2](state-directory.md#STATE-2) makes a profile directory nothing names a state this program
supports rather than an error, so every subsystem treats the absence as absence and none of them says
a word about it. A report that left it unsaid would describe a machine which works once and forgets as
a healthy one: the `settings` line above says at most that no file was found, which reads as a file
nobody has written rather than a directory there is nowhere to put.

Which variable answered is worth a few words for the same reason the settings files are named: more
than one can state a profile directory, and somebody moving the directory has to change the one in
force rather than the one they assume. Where none answered, naming every variable that was looked at
is what makes the remedy actionable, since which variables a platform states a profile directory in is
not something the reader is expected to know.

Whether the files under it are restricted to one account is a fact about the platform rather than
about this machine's configuration, and it is still owed. On Unix every file is created with a mode no
other account can read, and where there is no mode to ask for the same files carry what the profile
directory grants them; the prompt history is every path, branch name and pasted fragment somebody has
typed, and a profile on a shared or synced volume is where the difference lands. Nothing else in the
report distinguishes the two, so somebody about to type a token into a prompt has no other way to
learn which they have.

Both halves are named because the absence is partial, and which half is which cannot be worked out
from the report otherwise. A checkout's own settings, skills and `AGENTS.md` are read with no home at
all; the same files of the user's own, the session records behind `--resume`, the prompt history and
the recorded model and theme are not. A report naming only the loss would have somebody looking for
why the file in front of them is ignored when it is in force.

Failing on it is the wrong answer to the same fact: a container or a daemon with no profile directory
runs as designed and wants none of what it is not getting, so what is owed there is a sentence rather
than an error. The section sits outside the configuration one because a configuration error stops that
section before it prints anything, and where state is kept is a fact about the machine either way.

The network is reported for the reason the state directory is, one step further out again. The
certificate authorities a handshake is put to and the proxy a request crosses are stated outside this
program ([NET-7](network-egress.md#NET-7), [NET-8](network-egress.md#NET-8)) and appear in no other
line, and between them they account for the connection failure that has nothing to say for itself:
an authority the machine trusts and this build does not, or a route out nobody reading the rest of
the report would know was in use. Where nothing names either, the variables that would are named,
because which variables a machine states them in is not something the reader is expected to know.
The proxy is named without the credential it carries, and the hosts it is not used for are named
beside it, since those decide whether it applies to the host that is failing.

Four of the things it can say are configuration errors rather than findings, and make the command
fail: a named path that yielded no certificate, a set of roots that leaves nothing trusted, a
proxy named in a protocol this build cannot connect through, and a configuration naming nothing
that will serve a turn. Each is a statement about the machine that the program is not honouring,
which is the case a report passing with a warning would leave somebody to discover at the next
request.

The last of the four is where the report and the session have to agree. A configuration naming only
no model service is one a session refuses to open on ([BACKEND-39](backends.md#BACKEND-39)), and
the three ways to configure one are what the report says, in the same words. A report calling that machine
healthy would be read before anything else by the one person certain to run this command, which is
whoever was just refused.

The development section asks the same question one step in rather than one step out: not what this
machine will use, but whether this checkout is set up to be worked on. The links `agents/setup.py`
writes are gitignored, so a fresh clone and every new worktree start without them and nothing else
says so, which leaves an agent reading no instructions from the repository and a checkout that read
none looking exactly like one that did. `direnv` is where the build gets its configuration, and
without it a build fails naming a variable rather than the tool that would have set it. Neither is
guessable from the symptom and both are one command from fixed, which is what earns them a line.

Reported rather than failed on, because a checkout missing either still runs: a non-zero status
would call a machine holding everything the program needs a broken one. Shown only in a source
checkout for that argument from the other side, since a released binary needs neither, and a remedy
naming a script the reader does not have is noise in the one report people paste into issues.
Discovery stops at the nearest checkout boundary because a workspace of somebody's own can sit below
this one, and a report walking past its root would answer about a checkout they are not working in.

Nothing is repaired for the reason nothing else here is: somebody runs this to learn what is wrong,
and a report that fixes what it finds leaves them unable to tell what was already true. The link is
read for what it resolves to rather than for whether it exists, because a link to the wrong file is
the case a directory listing calls healthy, and it is the one the reader cannot otherwise catch.

`verified-by: bravebot_cli::main::a_gateway_credential_is_reported_as_found_and_never_printed`
`verified-by: bravebot_cli::main::a_gateway_with_no_credential_is_reported_as_having_none`
`verified-by: bravebot_cli::main::doctor_names_the_state_directory_it_resolved`
`verified-by: bravebot_cli::main::doctor_says_when_the_files_are_left_unrestricted`
`verified-by: bravebot_cli::main::a_missing_state_directory_is_reported_with_what_it_costs`
`verified-by: bravebot_cli::main::a_missing_state_directory_names_every_variable_it_looked_at`
`verified-by: bravebot_cli::main::the_network_section_names_the_roots_in_force_and_the_proxy`
`verified-by: bravebot_cli::main::the_network_section_points_at_the_variables_when_nothing_names_a_root_or_a_proxy`
`verified-by: bravebot_cli::main::a_trust_root_that_cannot_be_read_is_reported_as_the_reason_connections_will_fail`
`verified-by: bravebot_cli::main::doctor_names_what_the_managed_layer_pinned_and_the_file_it_came_from`
`verified-by: bravebot_cli::main::doctor_says_nothing_about_a_managed_layer_that_is_not_there`
`verified-by: bravebot_cli::main::doctor_names_a_managed_file_that_pinned_nothing`

<a id="CLI-8"></a>
### CLI-8: `--mode` chooses how a one-shot is run; the default is the turn loop

`turn` observes and decides step by step, which is what an unqualified `bravebot "task"` has
always been. `manifest` plans the whole run first, then executes it. An unknown name is refused
rather than guessed. Both modes carry an empty trust map, and every prompt a step raises is refused
unless the flag in CLI-1 says otherwise: those are due partway through a run, over a path or a
program nobody undertook to watch for, and typing a command is no undertaking to still be there.

A plan is the one question a one-shot answers, because it is asked at a moment the others are not:
once, before the first step, while nothing has been printed but this run's own progress. It is put
where both stdin and stderr are a terminal, which is where whoever typed the command is still
whoever is reading the output. Either end piped or redirected is nobody: a plan written into a file
is a plan nobody read, so it is refused as everything else is, and a scripted `manifest` run stops
before its first step unless the flag was given ([manifest.md](manifest.md#MANIFEST-10)).

The steps are the plan the run narrates a moment earlier, a line each from the renderer the question
was built from, so they are on screen while the question is answered and it does not print them
again. What the question adds is the task in the person's own words, how many steps they are
answering for, and what a yes does not cover.

This is a different axis from the mode in [permission-modes.md](permission-modes.md), and the two
compose. `--mode` decides when control flow is settled; the other decides who answers a prompt.

A failed plan is printed on stderr even without `--trace`, because otherwise a one-line complaint
is all that remains of a document nobody can see. The plan never shares stdout with the reply.

`verified-by: bravebot_cli::main::the_default_mode_is_the_turn_loop`
`verified-by: bravebot_cli::main::a_leading_mode_flag_is_a_task_not_an_unknown_option`
`verified-by: bravebot_cli::main::an_unknown_mode_is_refused_rather_than_guessed`
`verified-by: bravebot_cli::main::a_failed_plan_is_printed_beside_the_reply`
`verified-by: bravebot_agent::manifest::a_plan_nobody_approved_runs_nothing`
`verified-by: bravebot_cli::main::a_plan_is_answered_by_whoever_typed_the_command`
`verified-by: bravebot_cli::main::the_question_does_not_reprint_the_narrated_plan`
`verified-by: bravebot_cli::main::a_plan_is_refused_where_nobody_can_be_asked`
`verified-by: bravebot_cli::main::anything_but_yes_declines_a_plan`
`verified-by: bravebot_cli::main::a_one_shot_answers_the_plan_and_nothing_else`

<a id="CLI-9"></a>
### CLI-9: a one-shot run names its own model, or asks for the one a session would

`--model <name>` names the model for one run and outranks everything else. Where no flag names
one, the model is the one a session opening in the same directory would ask for: the choice
`/model` recorded, then the configured model, which is an exported
`BRAVE_AI_CHAT_DEFAULT_MODEL`, then the settings file's `model` key, then the default the build was
made with. A name is resolved against the configuration wherever it was written: `opus`, `sonnet`
and `haiku` name the tier's own model and the older spelling of the routing entry names the current
one, so the flag and the settings key it outranks accept the same spellings. A `--model` with no
name after it, or a blank one, is refused rather than read as no choice.

**Why.** A script that cannot name a model has only one route to a particular one, which is for
somebody to open the interface and pick it, and in a pipeline that is not a route at all. The flag
is that route, and it ranks above every other because it names a model for one invocation and
nothing else: two scripts in the same checkout can ask for different models, which nothing a file
records can do.

Below the flag, a run resolves a model the way a session does, so the two surfaces reach the same
models by the same names and a script needs no interactive step to use the one somebody already
chose. Ranking configuration above the record instead would mean a person who picked a model could
not run a script with it, and a script wanting a different one from the picked one has the flag.

Resolving against the configuration rather than at parse, because a tier word names a model only
the configuration knows: the AWS account's own model for that tier where it named one, and Brave's
name for it otherwise. A flag that sent such a word as written would refuse a spelling the file it
overrides takes, and be answered by whatever the service substitutes for a name it has never heard
of.

A blank name is refused because a script that computed an empty variable asked for a model.
Reading the blank as no choice would answer it with whatever was recorded or configured and say
nothing about having done so, which is the substitution the flag exists to make impossible.

`verified-by: bravebot_cli::main::a_model_flag_names_the_model_a_run_asks_for`
`verified-by: bravebot_cli::main::a_run_that_named_no_model_names_nothing`
`verified-by: bravebot_cli::main::the_command_line_outranks_the_record_a_session_would_read`
`verified-by: bravebot_cli::main::a_run_that_named_no_model_reads_the_record_a_session_would`
`verified-by: bravebot_cli::main::a_run_with_nothing_to_go_on_leaves_the_configured_model_in_force`
`verified-by: bravebot_cli::main::a_model_name_is_carried_as_it_was_typed`
`verified-by: bravebot_cli::main::a_tier_word_on_the_command_line_names_the_model_the_settings_key_would`
`verified-by: bravebot_config::lib::a_name_from_anywhere_resolves_as_the_settings_key_does`
`verified-by: bravebot_cli::main::a_model_flag_with_no_name_is_refused`
`verified-by: bravebot_cli::main::a_blank_model_is_refused_rather_than_read_as_no_choice`

<a id="CLI-10"></a>
### CLI-10: a substituted model is reported, and one the command line named fails the run

Where the endpoint answers with a model other than the one in force, both names are said on
stderr. Where the model in force is the one `--model` named, the run also exits non-zero. The reply
still goes to stdout, and stdout carries nothing else. Two cases are neither reported nor failed: a
name that asks for whichever model the server picks rather than for a particular one, and a backend
that does not report the name it was asked for.

**Why.** A model a run cannot be served is substituted rather than refused. One that needs a
subscription is answered by a weaker model, with an ordinary reply and nothing to distinguish it, so
the name the server reports is the only trace there is. Reporting it is about the model in force
rather than the flag alone, because every route to a model is somebody naming
one they expect to be answered by: the settings file's key is what a repository commits beside its
scripts, and a remembered choice is what a person picked and is being shown.

Failing the run is narrower, and the flag is what draws the line. A script that named no model
takes whatever was recorded or configured, so failing there would have it exit non-zero over a
choice made in a terminal it has nothing to do with, and a run that named one asked for something
and did not get it. The status is the part of a finished run a script is certain to read, which is
what makes it the thing that has to carry that, and the flag is what a script that cannot tolerate
a substitution has.

The two exclusions are the cases where a different name is not a substitution. A routing entry
resolves to a model per request, which is what it is for. A backend asked by an opaque handle
answers with a name that never matched what went in, so comparing them would fail every run made
against one.

`verified-by: bravebot_cli::main::a_model_asked_for_and_not_served_is_reported`
`verified-by: bravebot_cli::main::a_model_that_answered_as_asked_is_no_complaint`
`verified-by: bravebot_cli::main::a_routing_entry_answered_by_a_model_is_not_a_substitution`
`verified-by: bravebot_cli::main::a_backend_that_does_not_report_what_it_was_asked_is_not_compared`
`verified-by: bravebot_cli::main::a_substituted_model_is_reported_beside_the_reply_never_in_it`
`verified-by: bravebot_cli::main::a_run_answered_by_a_model_other_than_the_one_it_named_does_not_succeed`
`verified-by: bravebot_cli::main::a_substitution_the_command_line_did_not_ask_for_is_reported_and_not_failed`

<a id="CLI-11"></a>
### CLI-11: `--add-dir` makes a directory reachable, and vouches for nothing

`--add-dir <path>` opens a directory outside the working one for the length of the run, and may be
given more than once. An absolute path that exists, is a directory, and is not already inside the
working one is opened; anything else is refused by name and the run stops before the turn. The
run's trust map stays empty, so a file read there is read on the same footing as the project's own
files: nothing vouched for it. A write there is refused as any other write in an unattended run is,
and the flag in CLI-1 lifts that exactly as it does elsewhere.

**Why.** A headless task pointed at one checkout often needs to read another, and an absolute path
outside the working directory is otherwise refused whatever else is true, so without this the task
cannot be done at all.

Vouching is a separate grant, and it is the one an unattended run cannot make. The interactive
command of the same name records that a person vouched for the directory, which it can do because a
person typed it in a session whose map already holds their answer about the directory they are
working in. A run nobody is watching holds no such answer, its own working directory included, so a
rule trusting the tree named on the command line would leave it more trusted than the tree the run
works in. Reaching a directory is what the work needs; trusting what is in it is not.

Stopping rather than carrying on, because the two audiences differ: a session says the path was not
opened and leaves the person to retype it, and a script that carried on would fail somewhere further
in, over a file it was told it could open.

`verified-by: bravebot_cli::main::a_directory_flag_names_a_directory_the_run_may_reach`
`verified-by: bravebot_cli::main::the_directory_flag_is_repeatable`
`verified-by: bravebot_cli::main::a_directory_flag_with_no_path_is_refused`
`verified-by: bravebot_cli::main::a_directory_the_command_line_named_is_reachable`
`verified-by: bravebot_cli::main::a_directory_that_cannot_be_opened_stops_the_run`
`verified-by: bravebot_core::trust::an_empty_store_trusts_nothing`

<a id="CLI-12"></a>
### CLI-12: `--json` puts one result object on stdout, in the reply's place

A run given the flag writes one object, on one line, whether it finished, failed before the turn
began, or was refused something along the way. It holds how the run ended, the status and
identifier of CLI-6, the message where there is one, the reply, the model that answered, how many
rounds it took, what it cost in tokens, every tool it called with what it acted on and whether that
call was refused, and every refusal with the principle it upholds. A tool is named as the driver
matched it rather than by the word a person is shown. What a call acted on is the name it was given
rather than a resolved path, since the driver carries that argument without reading it.

The object takes the reply's place on stdout and nothing else goes there. Progress, the message and
the trail stay on stderr, exactly as they are without the flag.

It carries a schema number. Within one number a field may be added, and never removed, renamed or
given a different meaning, so a caller reading the fields it knows keeps working.

**Why.** The prose reply is written for a person, and a program can recover almost nothing from it:
which files changed, what the turn cost, which tools ran and why an effect was refused are either
absent or recoverable only by reading English that changes with the reader's language. Distinct
statuses say which kind of failure a run had; this says what happened in it, which is the other
half of being able to act on a result.

A separate flag rather than a replacement, because the prose contract in CLI-5 is right for the
person who typed the command, and a surface that served both would serve neither.

Written on every run rather than only on the ones that got as far as a turn, and a run that stopped
part way through still says what it had done by then. A caller that had to tell an empty stdout from
a result would be back to deciding from the shape of the output, which is the thing this removes,
and a run reporting nothing about calls it had already made would be worse than saying nothing at
all.

The schema number is what makes the object an interface rather than a rendering. A consumer in a CI
job is code somebody else wrote against fields this program chose, and without a stated rule about
what may change, every field is either frozen by accident or broken without warning.

`verified-by: bravebot_cli::running::a_run_asked_for_a_result_object_puts_one_on_stdout`
`verified-by: bravebot_cli::json::a_finished_run_says_what_it_did_in_fields_a_program_can_read`
`verified-by: bravebot_cli::json::a_failure_before_the_turn_is_still_a_result_object`
`verified-by: bravebot_cli::json::a_refusal_names_the_principle_it_upholds`
`verified-by: bravebot_cli::json::content_cannot_break_out_of_the_object_it_is_written_in`

<a id="CLI-13"></a>
### CLI-13: `--settings` names a file that outranks every layer found

`--settings <path>` reads one more settings file, above the three that
[backends.md](backends.md) resolves, for the length of the run. It resolves as those do, a name at
a time, so a file setting one value leaves the rest of what a person and a checkout configured in
force. The flag and its path are taken out of the arguments before anything dispatches on them, so
it composes with every way of starting and with the other two flags that are taken out there. Given
twice, the last file is the one read, and a path naming a file that is already one of the three is
read once. A path naming no file, and a path that is blank, are refused by name and the run stops
before it starts, with the result object of CLI-12 where one was asked for.

**Why.** The three layers that are found are properties of a person, of a checkout and of a machine.
None of them is a property of one invocation, so configuring one run differently from the next means
editing the home directory or the checkout, and a CI job or somebody holding two accounts can do
neither. That is the case for a flag, and there is nothing else it could be: a settings file is read
before a turn exists, so nothing inside a session can name one.

Above all three because naming a file is a stronger statement than a file being found where one was
looked for. A fourth layer rather than a replacement for them, because a job that wants one key
changed would otherwise lose the configuration the checkout carries, which it wants as well, and
would have to restate a whole configuration to move a profile.

Refused rather than ignored, on CLI-11's argument about two audiences: a run told to configure
itself from a file is a run whose configuration is that file, so carrying on under whatever the
directory happened to hold is the wrong configuration used in silence. A mistyped path and a
variable that expanded to nothing look the same from here, and both are ordinary.

What is checked is that the file is there, which is the mistake a command line makes. What is in it
is read by the rule [backends.md](backends.md) states for every layer, where one that is oversized
or unparseable leaves the others in force, and `doctor` lists the layers it read, so a named file
that did not parse is visible by its absence from that list.

`verified-by: bravebot_cli::main::the_settings_flag_is_taken_out_with_the_file_it_named`
`verified-by: bravebot_cli::main::a_named_settings_file_leaves_every_other_way_of_starting_intact`
`verified-by: bravebot_cli::main::the_last_settings_file_named_is_the_one_read`
`verified-by: bravebot_cli::main::a_settings_flag_with_no_path_is_refused`
`verified-by: bravebot_cli::main::a_named_settings_file_composes_with_the_other_flags_before_dispatch`
`verified-by: bravebot_cli::running::a_settings_file_named_on_the_command_line_is_read_above_the_ones_found`
`verified-by: bravebot_cli::running::a_refused_argument_exits_non_zero`
`verified-by: bravebot_cli::running::a_refused_settings_file_still_answers_with_a_result_object`
`verified-by: bravebot_config::settings::a_command_line_file_that_is_already_a_layer_is_read_once`
`verified-by: bravebot_config::settings::a_file_the_command_line_named_beats_every_layer_that_was_found`
`verified-by: bravebot_config::settings::a_name_a_command_line_file_left_alone_keeps_the_answer_below_it`
