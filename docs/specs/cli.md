---
id: CLI
title: The command line
status: normative
governs:
  - crates/cli/src/main.rs
  - crates/cli/src/auth.rs
  - crates/cli/src/completion.rs
  - crates/cli/src/continued.rs
  - crates/cli/src/exit.rs
  - crates/cli/src/json.rs
  - crates/cli/src/plain.rs
  - crates/config/src/keys.rs
  - crates/tui/src/hidden.rs
documented-by:
  - docs/website/docs/reference/cli.md
  - docs/website/docs/customize/signing-in.md
  - docs/website/docs/using/headless.md
---

## Scope

Running bravebot without the interface that draws: a one-shot task, piped input, `doctor`, `auth`,
a session in lines, and what goes where on the way out. The interface that draws is
[terminal-input.md](terminal-input.md) and [terminal-transcript.md](terminal-transcript.md).

A one-shot run has nobody to ask, and most of what makes it different follows from that. A session
in lines (CLI-14) has somebody, and everything that makes *it* different follows from the terminal
it does not take.

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
CLI-12, and it is still the only thing on it. The events of CLI-24 are the second exception: a run
given `--json-stream` writes them ahead of that object, and nothing else is on the stream.

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
`verified-by: bravebot_cli::running::doctor_ends_on_the_configuration_status_and_says_its_identifier`
`verified-by: bravebot_cli::running::doctor_ends_on_the_configuration_status_where_nothing_will_serve_a_turn`
`verified-by: bravebot_cli::main::a_report_that_found_a_configuration_error_ends_on_it`
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
confinement available on this platform, whether the network is closed for confined programs and
which layer closed it ([SANDBOX-20](sandboxing.md#SANDBOX-20)), the sandbox mode in force and the file
or flag that chose it ([SANDBOX-22](sandboxing.md#SANDBOX-22)), each entry of the four filesystem lists
with the file or flag that wrote it and each that is not in force with why
([SANDBOX-25](sandboxing.md#SANDBOX-25)), and the state of any imported subscription. Each AWS
account gets its profile and a line saying whether the AWS CLI gives it a credential a request can
be signed with. Where it does not, the line says why: a session that is signed out names `bravebot
auth login bedrock`, and a profile the CLI does not have, a CLI that is not installed and an answer
that is not a credential are each said as such, naming no sign-in. That line does not change the
exit status. Where the report names a Bravebot command that signs in, or one that forgets an
imported subscription, it is `bravebot auth login` or `bravebot auth logout`
([CLI-18](#CLI-18)). The signing key
is named as never transmitted, and a value from a settings file is never printed: where a credential
decides whether a backend works, what is reported is that one was found. A configuration error makes
it fail rather than pass with a warning.

The state directory is reported with the variable that named it, and on a platform where the files
under it cannot be restricted to one account the report says which of them carry the permissions of
the profile directory instead. Where there is no state directory, the report says which variables were
looked at, points the remedy at those same variables, names what is not kept without one, and says
that a checkout's own settings, skills and instructions are read regardless. It is reported rather than failed on, and sits outside the
configuration section, which a configuration error stops early.
Beside it, the report names the directory the diagnostic log is kept in ([diagnostic-log.md](diagnostic-log.md)).

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

`bravebot doctor --sandbox-check` runs the everyday workflows under the sandbox default instead of
reporting configuration ([SANDBOX-21](sandboxing.md#SANDBOX-21)). It takes no other argument, and
`--agent` and `--system-prompt` are refused with it as with `doctor`. It ends on the failed status
when a workflow failed, and on success when each worked or was skipped for a program that is not
installed.

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

An AWS account is the case where the profile line says nothing about that, since the credential is
the AWS CLI's and a session that has expired looks the same as one that works. The question is put
to the AWS CLI as a turn puts it before signing, so the answer is the one a session would get, and
only the answer is printed because what the CLI exports is a live credential. Asking can make the
AWS CLI renew its own cached session or prompt for an MFA code, as it does before a turn; nothing of
Bravebot's changes. A long-lived access key that AWS has revoked still reads as signed in, because
only a request to AWS would tell. It costs most of a second per account. The reasons a session is
not good are told apart because signing in fixes only an expired one: named beside a missing CLI or
profile, it sends somebody to a sign-in that fails the same way. None of them fails the report. A
signed-out session signs in on its first turn, and the others are the AWS CLI's to fix, which the
line says.

The commands the report names are `auth`'s because it is the one command that lists every way to
sign in. The older commands still work.

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
`verified-by: bravebot_cli::running::doctor_says_whether_each_aws_account_is_signed_in_and_never_the_credential`
`verified-by: bravebot_cli::running::doctor_names_no_sign_in_for_a_missing_aws_cli_or_an_unknown_profile`
`verified-by: bravebot_cli::running::doctor_ends_on_the_configuration_status_where_nothing_will_serve_a_turn`
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
`verified-by: bravebot_cli::main::doctor_names_the_sandbox_mode_and_where_it_came_from`

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
one, the model is the one a session opening in the same directory would ask for, in the order
[BACKEND-11](backends.md#BACKEND-11) gives: a `model` key in a checkout's settings or the file
`--settings` named, then the choice `/model` recorded, then the key in the person's own settings,
then an exported `BRAVEBOT_DEFAULT_MODEL`, then the default the build was made with. A name is
resolved against the configuration wherever it was written: `opus`, `sonnet` and `haiku` name the
tier's own model and the older spelling of the routing entry names the current one, so the flag and
the settings key it outranks accept the same spellings. A `--model` with no name after it, or a
blank one, is refused rather than read as no choice.

**Why.** A script that cannot name a model has only one route to a particular one, which is for
somebody to open the interface and pick it, and in a pipeline that is not a route at all. The flag
is that route, and it ranks above every other because it names a model for one invocation and
nothing else: two scripts in the same checkout can ask for different models, which nothing a file
records can do.

Below the flag, a run resolves a model the way a session does, so the two surfaces reach the same
models by the same names and a script needs no interactive step to use the one somebody already
chose. Where the checkout it runs in names a model, that is the one it gets, as a session opened
there would: a pick is one per person and says nothing about which checkout it was made for, and a
script wanting a model other than the checkout's has the flag.

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
`verified-by: bravebot_cli::running::a_run_asks_for_the_recorded_model_ignoring_a_checkouts`
`verified-by: bravebot_cli::running::a_run_asks_for_the_settings_model_over_an_exported_default`
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
identifier of CLI-6, the message where there is one, the reply, the model that answered, the
definition the turn was addressed to where there was one (CLI-17), the session it wrote down where
it wrote one (CLI-25), how many rounds it took, what it
cost in tokens, every tool it called with what it acted on and whether that call was refused, and every refusal with the principle it upholds. A tool is named as the driver
matched it rather than by the word a person is shown. What a call acted on is the name it was given
rather than a resolved path, since the driver carries that argument without reading it.

A failed run retains measured cumulative token usage from completed work. The latest report
replaces earlier reports; a successful outcome is charged once. Required numeric token fields
remain zero when no measurement exists. Failure reports do not invent a model, reply or round
count.

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

`verified-by: bravebot_cli::running::failed_json_retains_planner_and_vetting_usage`
`verified-by: bravebot_cli::running::json_usage_controls_do_not_double_charge_or_guess`
`verified-by: bravebot_cli::running::a_run_asked_for_a_result_object_puts_one_on_stdout`
`verified-by: bravebot_cli::running::a_refused_command_line_asking_for_a_result_object_gets_one_instead_of_the_usage`
`verified-by: bravebot_cli::json::a_finished_run_says_what_it_did_in_fields_a_program_can_read`
`verified-by: bravebot_cli::json::a_failure_before_the_turn_is_still_a_result_object`
`verified-by: bravebot_cli::json::a_refusal_names_the_principle_it_upholds`
`verified-by: bravebot_cli::json::content_cannot_break_out_of_the_object_it_is_written_in`
`verified-by: bravebot_cli::running::a_run_under_a_definition_names_it_in_the_result_object`
`verified-by: bravebot_cli::json::a_recorded_run_names_its_session`

<a id="CLI-13"></a>
### CLI-13: `--settings` names a file that outranks every layer found

`--settings <path>` reads one more settings file, above the three that
[backends.md](backends.md) resolves, for the length of the run. It resolves as those do, a name at
a time, so a file setting one value leaves the rest of what a person and a checkout configured in
force. The flag and its path are taken out of the arguments before anything dispatches on them, so
it composes with every way of starting and with the other flags that are taken out there. Given
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

<a id="CLI-14"></a>
### CLI-14: `--plain` is a session in lines, and takes nothing from the terminal

The flag starts an ordinary session, with the same turns, the same conversation carried between
them, the same trust map and the same questions, and with nothing drawn. None of what the interface
that draws takes ([terminal-input.md](terminal-input.md)) is taken here: no raw mode, no screen of
its own, no mouse reporting, no bracketed paste, no focus reporting and no keyboard enhancement. So
what the terminal held before is where it stays, the session's own lines are added to its
scrollback, nothing is repainted, and nothing moves on its own.

A line typed is a prompt, and Enter sends it. A blank line is not a prompt. The end of the input
ends the session, which is the only way out of it: no chord is read, because none can be. The
keyboard is the terminal's, so its own interrupt ends the process and its own end of file ends the
session.

stdin must be a terminal, and `--plain` is refused where it is not, naming `-p` as the invocation
that reads a pipe. The reply goes to stdout and everything else to stderr, which is CLI-5's
division, so a session in lines is as pipeable as a one-shot run.

Every question is put the same way: what it is about, a line at a time, then the question, then how
to answer it. Only the affirmative approves. Any other line refuses, and so does the end of the
input. One answer per question and no second key, so the answers that record something, stop asking
about these programs and remember this line, are not offered, and nothing answered here outlives
the session. The startup question about the working directory
([trust-map.md](trust-map.md)) is put the same way, and the end of the input in place of an answer
to that one starts no session at all. It is not put where the person told the interface that draws,
or the desktop, to remember the answer about this directory
([TRUST-23](trust-map.md#TRUST-23)): that record is read here as it is there, and the session says
it is trusting the directory for that reason, when, and how to be asked again. Remembering is still
not offered here, and nothing here writes the record.

It composes with `--incognito`, `--dangerously-skip-permissions` and the `--settings` file of
CLI-13, which belong to every way of starting, with the `--agent` of CLI-17, and with nothing else:
it starts a session rather than describing one.

**Why.** A viewport repainted in place is not a document a screen reader can follow, and what
leaves the top of it is in this program's own scroller rather than in the terminal's scrollback,
where a person's own tooling knows how to look. [terminal-input.md](terminal-input.md) says what
the interface takes and why each part of it is needed; the answer to somebody who cannot use the
result is not a smaller version of the same thing, it is a session that takes none of it.

A line rather than a panel for every question, because the panel is the takeover: a box drawn over
a transcript needs the screen the transcript is on. What a line loses is the second and third key,
the ones that grant something standing, and that is the right thing to lose here rather than to
spell as more letters after `y`. A standing permission granted by a mistyped character cannot be
taken back, and saying yes again next time costs a keystroke.

Prompts are read only from a terminal because they are the trusted input, and what CLI-3 settles is
that a pipe carries bytes nothing vouched for. A session reading prompts from a pipe would take its
instructions from whatever fed it, and answer its own approval questions out of the same bytes,
which is the whole guarantee inverted for the sake of a convenience `-p` already provides.

**What it does not have.** Everything that was a drawing: the scroller and its search, the key
list, the slash commands, `@` naming a file, a picture on the clipboard, the audit trail under a
key. No session record is written either, so nothing picks a session in lines up again, and a
`--resume` reached for afterwards will not find it. Each of those is a thing the interface draws or
a thing that needs what it draws, and a session in lines is the turns without them.

**A known cost.** A line typed before a question was asked is read as the answer to it. The
terminal queues what is typed and this reads a line at a time, so somebody who pastes several lines
at once has typed all of them before anything asked them anything, and a question raised while
those lines are still queued takes the next one as its answer. Only the affirmative approves, so
the line has to be exactly that word for an effect to follow, and every other line refuses. The
interface that draws is not exposed to this because bracketed paste tells it where a paste begins
and ends, which is one of the modes this mode does not take: reading the queue ahead of a question
needs the terminal put in a state a session in lines does not put it in.

`verified-by: bravebot_cli::plain::a_session_in_lines_asks_the_terminal_for_nothing`
`verified-by: bravebot_cli::plain::the_reply_is_the_only_thing_on_the_reply_stream`
`verified-by: bravebot_cli::plain::the_end_of_the_input_ends_the_session`
`verified-by: bravebot_cli::plain::a_blank_line_is_not_a_turn`
`verified-by: bravebot_cli::plain::a_failed_turn_says_so_beside_and_the_session_goes_on`
`verified-by: bravebot_cli::plain::only_the_affirmative_approves_and_silence_refuses`
`verified-by: bravebot_cli::plain::a_substituted_model_is_said_beside_the_reply`
`verified-by: bravebot_cli::plain::a_write_is_asked_about_with_the_change_it_would_make`
`verified-by: bravebot_cli::plain::a_write_a_processor_produced_carries_what_it_said_about_it`
`verified-by: bravebot_cli::plain::a_run_is_asked_about_one_argument_to_a_row`
`verified-by: bravebot_cli::plain::the_startup_question_is_asked_in_lines_and_answered_the_same_way`
`verified-by: bravebot_cli::plain::the_mode_that_asks_about_nothing_is_not_asked_about_the_directory`
`verified-by: bravebot_cli::plain::a_remembered_answer_settles_a_session_in_lines_without_asking`
`verified-by: bravebot_cli::plain::an_answer_in_lines_is_asked_for_and_never_kept`
`verified-by: bravebot_cli::running::a_session_in_lines_is_refused_where_its_input_is_not_a_terminal`
`verified-by: bravebot_cli::main::a_named_settings_file_composes_with_the_other_flags_before_dispatch`
`verified-by: bravebot_cli::main::the_agent_flag_is_taken_out_with_the_name_it_gave`

<a id="CLI-15"></a>
### CLI-15: `--vet` lets a check that finds nothing answer, for this run

`--vet` turns auto-vetting on for the length of the run: where a check completes and finds nothing,
the slot the planner asked to be shown, or the output it asked to read back, is promoted without a
prompt.
[vetting.md](vetting.md#CHECK-12) is what that covers and what it does not, and
[vetting.md](vetting.md#CHECK-11) is the other two routes in and how they resolve against this one.

**The other half of what it says, where nobody can be asked.** On a run that also bypasses
permissions ([permission-modes.md](permission-modes.md#MODE-4)) the flag is what makes a verdict able
to refuse: those two prompts are otherwise answered yes unshown, and with the flag a word that is not
`safe` keeps the bytes back instead. Refusing covers a check that did not complete as well as one
that objected. So an unattended run is screened rather than unscreened, which is the only reading
under which asking for a check on a run with no prompts means anything.

What it does not buy on that run: the bytes still reach the backend in the confined conversation,
since making the check is what sends them, and each promotion costs a model call in the run's own
critical path. Both are the price of the screening rather than side effects of it, and a run that
wants neither leaves the flag off.

The flag is taken out of the arguments before anything dispatches on them, so it composes with
every way of starting and with the other flags taken out there, `--plain` included. Given
twice it is given once, which is asking for something that is already on rather than an error to
report.

It outranks both standing answers, a recorded `off` included, because it is the narrowest in time:
somebody typing it has said what they want of the run in front of them, and that is the footing
`--dangerously-skip-permissions` sits on, which is a strictly larger thing anything able to pass
this flag could pass instead. There is no flag the other way, which would matter only to somebody
who had turned the mode on standing and wanted one run without it; for them the answer is the file
the standing answer is kept in.

**Why a flag at all.** CLI-1 refuses everything nobody can be asked about, which makes a check on
this path a model call whose word ends in a refusal: the one-shot run has no prompt to fall back to.
So a run that wants the agent to use a fetched page has the two moves the whole of
[vetting.md](vetting.md) exists to add a third to, and on this path the third one needs saying in
advance. That is the same footing [permission-modes.md](permission-modes.md) sits on, and it is a
narrower statement than `--dangerously-skip-permissions`: it answers one question, about one slot at
a time, and only where a check completed and found nothing.

`verified-by: bravebot_cli::main::the_vet_flag_is_taken_out_wherever_it_appears`
`verified-by: bravebot_cli::main::asking_to_vet_twice_is_asking_once`
`verified-by: bravebot_cli::main::an_invocation_that_only_mentions_vetting_does_not_ask_for_it`
`verified-by: bravebot_cli::main::vetting_composes_with_the_other_flags_that_lead`
`verified-by: bravebot_core::vetting::asking_on_the_command_line_is_one_way_and_idempotent`

<a id="CLI-16"></a>
### CLI-16: a one-shot run names its own effort level, or asks for the one a session would

`--effort <level>` names how hard the model is asked to think for one run, and outranks every
settings file and the level `/effort` recorded. The word is one `/effort` takes, in any case. Where
no flag names one, the level is the one a session opening in the same directory would ask for, in
the order [BACKEND-43](backends.md#BACKEND-43) gives. A `--effort` with no word after it, a blank
one, or a word that is no level is refused before the run starts, and the refusal names the levels.
Nothing is recorded: the flag says what one run asks for.

**Why.** The level is the other half of what [CLI-9](#CLI-9) lets a script say about the model, and
without a flag a script's only routes to one, short of writing a file for `--settings`, are a file
in the checkout and a pick somebody made in the interface. Neither is the script's to state, and two
scripts in one checkout cannot want different levels through either.

A word that is no level is refused rather than read as no choice, for the reason CLI-9 refuses a
blank model and for a stronger one: a model name the service does not know is substituted and
reported ([CLI-10](#CLI-10)), where a word that is no level would be dropped here with nothing said,
and the run would go out at whatever level ranks next. Whether a level the flag named then goes out
at all is [BACKEND-22](backends.md#BACKEND-22)'s question, as it is for any other.

`verified-by: bravebot_cli::main::an_effort_flag_names_the_level_a_run_asks_for`
`verified-by: bravebot_cli::main::an_effort_flag_naming_no_level_is_refused`
`verified-by: bravebot_cli::running::a_run_sends_the_level_the_command_line_named_over_every_other`

<a id="CLI-17"></a>
### CLI-17: `--agent` addresses every turn of a session or a run to one definition

`--agent <name>` addresses every turn of an interactive session, a session in lines (CLI-14) or a
one-shot run to the definition `name` selects, as a `/agent` line addresses one for a single turn
([addressing-a-definition.md](addressing-a-definition.md)). The flag and its name are taken out of
the arguments before anything dispatches on them, as `--settings` is. If the flag is given twice,
the last name is used. It is refused when no name follows it, when the name is blank or opens with
`-`, and when it is given with `--mode manifest` or a command that starts neither a session nor a
task.

The name is matched against the set a turn starting now would resolve
([ADDRESS-5](addressing-a-definition.md#ADDRESS-5)), before any turn is sent. A session matches it
after the startup question about the directory ([trust-map.md](trust-map.md)) is answered, because
the set depends on that answer. A one-shot run does not ask that question, so its set holds the
built-in kinds and the person's own definitions, and it counts the definitions in a checkout's
`.bravebot/agents` without reading them. A name the set does not hold is refused with the names it
does hold. If the checkout held definitions the run did not read, the refusal says how many. A
definition whose model needs a sign-in this machine has not made is refused as well
([ADDRESS-11](addressing-a-definition.md#ADDRESS-11)). A refusal exits with the status for an
argument (CLI-6), writes the result object of CLI-12 if one was asked for, and sends nothing.

Every turn the session sends is addressed to the definition: typed lines, `/loop` ticks and `/goal`
rounds. The later look and the watch that a turn could otherwise arrange are still withheld
([ADDRESS-8](addressing-a-definition.md#ADDRESS-8)). A `/agent` line naming another definition
addresses that one for one turn, and the next turn goes to the session's definition again
([ADDRESS-10](addressing-a-definition.md#ADDRESS-10)). Replies are drawn under the definition's
name, `/status` names it, and the input box is unchanged.

A model the definition names is the session's model, so the context window, the effort level and
the model `/status` reports are that model's. `/model` is refused in such a session and says why,
and the model a person picked in an earlier session stays recorded.

On a one-shot run, `--model` outranks the definition's model, and the run says so on stderr. Stdout
is still only the reply (CLI-5), and the result object names the definition in `agent`. Without
`--model`, the check that some service is configured is made for the definition's model, since that
is the model the run asks for.

**Why.** A definition is a prompt, a model and a smaller set of tools that a person wrote. With only
`/agent`, the person has to name it on every line, and a line that leaves the name out runs with
every tool the session has. A `-p` run has no input line, so the flag is the only way to address a
definition there.

Only definitions a person vouched for are read, because every turn runs under the one selected. A
definition read from an untrusted checkout would put text from whoever wrote that checkout into the
prompt of a run nobody is watching ([DELEGATE-20](delegation.md#DELEGATE-20)). The refusal gives
the count so that a person who can see the file knows why it is not in the list.

Ticks and rounds are addressed too, because addressing only narrows what a turn can do
([ADDRESS-7](addressing-a-definition.md#ADDRESS-7)). An unaddressed tick would have more tools than
the turns the person typed.

The name is written into the record of the session it started, by the driver from this argument. A
session picked up with `--resume`, `--continue`, `--fork`, `--from-pr`, `/resume` or a one-shot run
carrying one on works under the recorded name, matched as above before any turn is sent
([ADDRESS-3](addressing-a-definition.md#ADDRESS-3)). A `--agent` given with one of them names the
definition for that session in place of the recorded one, and is refused as a fresh session's is
when it matches nothing. A recorded name that matches nothing now does not end the run: it is said,
with the definition named and that the narrowing is gone, and the session or run goes on as the
planner's and stops recording the name. A recorded name whose model needs a sign-in is refused, since
going on would substitute the planner's model. It is refused with `--mode manifest` because a manifest run plans every step before any runs and none of the steps is
addressed, while a definition is addressed one turn at a time.

`--model` outranks the definition for the reason in CLI-9: it names the model for this one
invocation, so a person who gives both flags has said which model this run should use. `/model` in
a session does not, because it comes after the session was started under the definition, and the
definition's model is part of that definition ([ADDRESS-11](addressing-a-definition.md#ADDRESS-11)).
Picking a model no turn asks for would only change what `/status` says.

A name opening with `-` is refused because it is the next flag. Taken as the name, `--json` would
be removed from the arguments and the run would answer in the other format. No definition's name
may open with `-`, so no name is lost.

**The `agent` setting.** Where no `--agent` is given, the `agent` key in the person's own settings
file or in the file `--settings` names stands in for it, and the flag outranks it
([ADDRESS-13](addressing-a-definition.md#ADDRESS-13)). A recorded name outranks it too. A name it
gives that matches nothing is said and the run goes on without one, where the flag's is refused.

**Known costs.** A self-paced `/loop` under a definition stops after one tick, because an addressed
turn cannot schedule the next one. A `/loop` with an interval keeps running. A checkout's settings
cannot choose the definition, so a checkout that always wants one has to be given it by each
person's own file or by `--agent`.

`verified-by: bravebot_cli::main::the_agent_flag_is_taken_out_with_the_name_it_gave`
`verified-by: bravebot_cli::main::the_last_definition_named_is_the_one_worked_under`
`verified-by: bravebot_cli::main::an_agent_flag_with_no_name_is_refused`
`verified-by: bravebot_cli::main::a_definition_is_refused_where_nothing_would_work_under_it`
`verified-by: bravebot_cli::running::a_run_under_a_definition_nobody_wrote_is_refused_with_the_names_that_exist`
`verified-by: bravebot_cli::running::a_run_refuses_a_definition_only_an_untrusted_checkout_holds_and_says_it_counted_one`
`verified-by: bravebot_cli::running::a_run_under_a_definition_is_offered_only_the_definitions_tools`
`verified-by: bravebot_cli::running::a_run_under_a_definition_names_it_in_the_result_object`
`verified-by: bravebot_cli::running::a_run_under_a_definition_is_checked_for_a_service_that_serves_the_definitions_model`
`verified-by: bravebot_agent::turn::a_model_the_command_line_named_outranks_the_definitions_and_the_turn_says_so`
`verified-by: bravebot_agent::turn::an_addressed_turn_arranges_no_later_look_and_arms_no_watch`
`verified-by: bravebot_tui::app::a_session_started_under_a_definition_addresses_every_turn_a_loop_tick_included`
`verified-by: bravebot_tui::app::a_definition_named_on_the_line_lasts_one_turn_under_the_one_the_session_works_under`
`verified-by: bravebot_tui::app::a_name_from_the_command_line_is_worked_under_where_it_was_written_and_refused_where_not`
`verified-by: bravebot_tui::app::a_session_under_a_definition_naming_a_model_works_on_it_and_refuses_the_picker`
`verified-by: bravebot_tui::status::the_report_names_the_definition_every_turn_is_addressed_to`

<a id="CLI-18"></a>
### CLI-18: `auth login` lists every way to sign in to a model service and runs the one picked

`bravebot auth login` with no way named lists the ways this program signs in to a model service,
each by a number and by the word that names it:

- `leo` imports a Brave Leo Premium subscription, as `import-leo-creds` does.
- `bedrock` signs in to every AWS account the configuration names for Amazon Bedrock.
- `import` reads model services out of other tools, as `import-providers` does.
- `gateway` stores a key for a gateway a provider block names.

A way that already holds a sign-in says so on its line. For Leo that is the line `doctor` prints
for the stored subscription. For Bedrock it is shown where every account named has a usable
session. For a gateway it is the ids keys are stored for. The credential itself is never printed.

The answer is a number or a name, in any case. If nothing is typed, or the input ends, the command exits
successfully having run nothing. An answer that names no listed way is refused with the argument
status (CLI-6). Picking Leo asks which channel, and nothing typed means stable. Where a subscription
is already imported, it first asks whether to sign in again, names `bravebot auth logout leo`, and
runs nothing without a yes.

`bravebot auth login <way>` runs that way without the list. A word after `leo` is the channel and
is checked as `import-leo-creds` checks it. A word opening with `-` is refused there, and so are a
second word and any word after `bedrock` or `import`. The list needs a terminal on stdin and on stderr. Without one, the
command is refused with the argument status and the forms a script can type instead.

Each way is run by the function its own command runs, so `auth login import` prompts, refuses and
writes where `import-providers` does. The Bedrock way signs in once per AWS profile: first the
account the tier variables name, then each `amazon-bedrock` provider block. An account naming no
profile is on the one `AWS_PROFILE` names, which the `aws` it starts inherits. Every account is
tried, past one that fails. One is reported signed in only where its credentials can be exported
after the sign-in. One that cannot is named, and the command exits with the failure status.

Picking the Bedrock way is the opt-in `BRAVEBOT_USE_BEDROCK=1` states. Where nothing sets the name,
the accounts are the ones the configuration names with it set, if a tier variable then names a
model, and the ones it names as it is otherwise. Once the tier variables' account signs in, the
command adds `"BRAVEBOT_USE_BEDROCK": "1"` to the `env` block of the person's own `settings.json`,
keeping what the file says, and says it did. Nothing is written where the machine-level layer sets
the name, where an export or a settings file sets it to something other than 1, where the person's
own file names it already, or where only a provider block's account signed in. A checkout's file
setting it to 1 does not stop the record. Where an export sets it to 1 over a settings file that sets
something else, the file is left as it is, and the command says a session uses Bedrock only with the
export. Where the person's own file names it with a value other than 1, the command says it left the
file as it is. Where the switch cannot be recorded, the command exits with the failure status: there
is no home directory, the file holds no settings document, or its `env` is not a block. Where the
configuration names no account, it exits with the configuration status and names `AWS_REGION` and
the tier variables. Where the name is set to something other than 1, it says that turns Bedrock off
instead, and names the machine-level file where that file is what sets it.

`bravebot auth login gateway [id]` stores a key for the gateway whose provider block has that id.
Bedrock blocks are not gateways here. With no id, one configured gateway is used, and several are
listed with the host each sends to, to be picked by number or id. Everything that needs no answer is
checked before the key is asked for, and each refusal writes nothing:

- an incognito session, with the failure status;
- no gateway configured, with the configuration status;
- an id no block has, naming the ids that are configured and not the word given, with the argument
  status;
- no terminal on stdin and on stderr, with the argument status;
- a file of keys that cannot be read, which is left as it is, with the failure status.

A word after the id is refused with the argument status and is not repeated, and an option is named
without what follows its `=`, since either is most likely the key
([CRED-24](credential-protection.md#CRED-24)). A key already stored for the gateway is replaced only
on a yes. Before the key is asked for, the command names a variable in the block's `env` that is
set, since that is sent instead ([BACKEND-16](backends.md#BACKEND-16)). The key is typed with
nothing drawn: Enter takes it, and Escape, Ctrl-C, Ctrl-D or Enter on nothing stores nothing and
exits successfully. The file is read again once the key is typed, so a key another command forgot
while this one waited is not written back. It is written to `gateway-keys.json` in the state
directory, created 0600 on Unix and granted to this account alone on Windows, beside the file,
flushed to disk and renamed over it. The command names the host the key is sent to.

In an incognito session the Leo and import ways are refused by the checks `import-leo-creds` and
`import-providers` make, with their words. The Bedrock way is not refused. It records nothing, and
says `BRAVEBOT_USE_BEDROCK=1` still has to be exported.
`bravebot auth logout leo` forgets the stored subscription, as `import-leo-creds --forget` does, and
is allowed in an incognito session. `bravebot auth logout gateway [id]` forgets the key stored for
that id, or the only one stored where none is named, and is allowed in an incognito session too.
With several stored and none named, or an id with none stored, it is refused with the argument
status and names the ids that hold one. The file is removed with its last key. It says the key still
works at the host its block names until it is revoked there. `auth logout bedrock` and
`auth logout import` are refused and say what to run instead, since neither way keeps a credential
of its own.

**Why.** Each service had its own route: two commands, an environment variable, and a session that
signs in to AWS on its first turn. Nobody who had not read the documentation could find them, and
`bravebot auth login` was refused as an unexpected argument. The list names the routes that exist
and shows which are already in use.

Each way calls the function its old command calls rather than a copy of it, so the refusals and
prompts cannot drift between the two, and nothing that scripts the old commands breaks. The command
runs before any session exists, so no planner or driver context is involved and no labelled value
passes through it.

A held Leo sign-in is confirmed before it is repeated because a second import registers this
machine with Brave as one more device. A repeated Bedrock sign-in leaves a good session alone, and
a repeated import says when it has nothing new, so neither is asked about. A stored gateway key is
confirmed before it is replaced, because the file is rewritten and nothing else holds the old one.

A gateway key is kept out of `settings.json` because a settings file is one people paste into issues
and copy between machines, and the program rewrites it for reasons that have nothing to do with a
credential. It is typed rather than passed as an argument because an argument is readable by other
programs on the machine and kept in the shell's history. The file holds an id and a key and no host,
so the block decides where the key goes, and a file of keys that will not parse is refused rather
than read as empty, because the next write would make it empty.

Leo, the import and the gateway key are refused in an incognito session for the reason in
[INCOG-7](incognito.md#INCOG-7): each writes to the directory this program owns. The AWS session is
the AWS CLI's, a subprocess [INCOG-8](incognito.md#INCOG-8) leaves outside the mode, and a session
in that mode signs in to it on its first turn, so refusing it here would refuse nothing the session
does not do. Signing out is allowed because it leaves less behind.

An account is signed in to once per profile because the AWS session belongs to the profile. A second
account on the same profile would check the same session again and report it twice. The check is
made again after `aws sso login`, because that command can finish for a profile whose credentials
still cannot be exported, and the export is what a turn signs with.

Picking Bedrock is read as the opt-in because an account signed in to that no session then uses is a
sign-in that did nothing. The switch names a destination, which a settings file may name
([BACKEND-1](backends.md#BACKEND-1)), and `import-providers` writes the same entry for an account it
imports ([IMPORT-3](import.md#IMPORT-3)). It goes in the person's own file, the lowest layer, so a
checkout's file or the machine-level layer that turns Bedrock off still does. A checkout's file
turning it on is read only in that checkout, so it is no record. The switch is read as on only where
a tier variable names a model: without one the tier variables' account is not one a session could
use, and an `AWS_REGION` exported for a provider block would bring a sign-in to the default AWS
profile nobody asked for. It is written only once the tier variables' account signs in. A provider
block's account needs no switch, and a switch written for an account that failed would turn on a
backend that cannot sign. A value a file already holds is never changed: a file turning Bedrock off
is somebody's decision, and an export over it is for that shell.

**Known costs.** A stored gateway key is read when the program starts, so a session already running
does not send one stored after it began. The key is held in a file the account can read, not in the
platform's keychain, which a later change can move it to. Two sign-ins storing keys at the same time
can lose one, since each rewrites the file. The Bedrock way records the switch and not `AWS_REGION`
or `AWS_PROFILE`, so a region exported only for the sign-in has to be exported for a session too.
The list asks the AWS CLI about each account before it is shown, which takes most of a second per
account whose session has not been checked yet.

`verified-by: bravebot_cli::auth::every_way_is_listed_and_only_a_held_one_says_what_it_holds`
`verified-by: bravebot_cli::auth::a_way_is_picked_by_its_number_or_its_name`
`verified-by: bravebot_cli::auth::leo_is_asked_which_channel_and_passes_the_word_on`
`verified-by: bravebot_cli::auth::a_held_leo_sign_in_is_repeated_only_when_asked_to`
`verified-by: bravebot_cli::auth::each_aws_profile_is_signed_in_to_once`
`verified-by: bravebot_cli::auth::the_switch_is_recordable_where_nothing_ranked_above_the_persons_file_turns_it_off`
`verified-by: bravebot_config::settings::a_default_answers_only_where_no_layer_names_it`
`verified-by: bravebot_cli::auth::a_gateway_is_picked_by_its_number_or_its_id`
`verified-by: bravebot_cli::auth::one_gateway_or_a_named_one_is_not_asked_about`
`verified-by: bravebot_cli::auth::a_stored_key_is_replaced_only_when_asked_to`
`verified-by: bravebot_cli::auth::the_key_forgotten_is_the_one_named_or_the_only_one`
`verified-by: bravebot_cli::auth::a_set_variable_is_named_as_the_one_sent_instead`
`verified-by: bravebot_cli::auth::stored_keys_are_written_private_and_read_back`
`verified-by: bravebot_cli::auth::the_padding_counts_an_id_in_characters`
`verified-by: bravebot_config::keys::a_stored_key_is_read_back_under_its_id`
`verified-by: bravebot_config::keys::a_file_that_is_not_one_of_keys_is_refused`
`verified-by: bravebot_config::keys::no_file_is_no_keys`
`verified-by: bravebot_config::keys::a_key_json_escapes_is_read_back_unchanged`
`verified-by: bravebot_tui::hidden::enter_takes_what_was_typed_and_backspace_takes_one_off`
`verified-by: bravebot_tui::hidden::escape_and_control_c_stop_without_a_key`
`verified-by: bravebot_tui::hidden::a_release_and_a_control_letter_type_nothing`
`verified-by: bravebot_tui::hidden::control_u_starts_the_key_again`
`verified-by: bravebot_tui::hidden::a_character_typed_with_altgr_is_part_of_the_key`
`verified-by: bravebot_tui::hidden::a_key_longer_than_the_room_made_for_it_is_kept_whole`
`verified-by: bravebot_cli::main::a_definition_is_refused_where_nothing_would_work_under_it`
`verified-by: bravebot_cli::running::auth_login_naming_no_way_is_refused_where_nobody_can_pick_one`
`verified-by: bravebot_cli::running::auth_refuses_what_names_no_way_to_sign_in`
`verified-by: bravebot_cli::running::auth_login_in_an_incognito_session_refuses_what_its_command_refuses`
`verified-by: bravebot_cli::running::auth_logout_leo_forgets_the_import_in_an_incognito_session`
`verified-by: bravebot_cli::running::auth_login_bedrock_is_refused_where_no_aws_account_is_configured`
`verified-by: bravebot_cli::running::auth_login_bedrock_signs_in_to_every_profile_and_names_the_one_that_failed`
`verified-by: bravebot_cli::running::auth_login_bedrock_records_the_opt_in_once_its_account_signs_in`
`verified-by: bravebot_cli::running::auth_login_bedrock_records_the_opt_in_where_only_a_checkout_turns_it_on`
`verified-by: bravebot_cli::running::auth_login_bedrock_signs_in_only_a_provider_block_where_no_tier_names_a_model`
`verified-by: bravebot_cli::running::auth_login_bedrock_in_an_incognito_session_records_nothing`
`verified-by: bravebot_cli::running::auth_login_bedrock_leaves_a_settings_file_it_cannot_record_in_as_it_is`
`verified-by: bravebot_cli::running::auth_login_import_is_the_import_and_refuses_where_it_does`
`verified-by: bravebot_cli::running::auth_login_gateway_is_refused_before_a_key_is_asked_for`
`verified-by: bravebot_cli::running::auth_logout_gateway_forgets_the_key_named_in_an_incognito_session`

<a id="CLI-19"></a>
### CLI-19: `--system-prompt` and `--append-system-prompt` put a person's own words in the planner's system prompt for one run

`--system-prompt <prompt>` replaces the opening of the planner's system prompt, the paragraph that
says what kind of assistant it is, and nothing after it. `--append-system-prompt <prompt>` adds the
words as the last standing source, after the project's `AGENTS.md` ([INSTR-10](instructions.md#INSTR-10)).
Each may be given without the other or with it. They apply to an interactive session, a session in
lines (CLI-14) and a one-shot run, and they combine with `--agent`, `--plain`, `-p`, `--json`,
`--resume`, `--continue` and `--fork`. Both are taken out of the arguments before anything
dispatches on them, as `--agent` is.

What stays when the opening is replaced is everything the opening is not: what teaches the planner to
treat what a tool returns as data, what a person is to be told, the facts about the machine, the
mode, the goal of a `/goal` round and the standing instructions. Those are not the person's to
rewrite with a flag, and the clauses that depend on them ([GOAL-14](goal.md#GOAL-14),
[INSTR-5](instructions.md#INSTR-5), [INSTR-9](instructions.md#INSTR-9)) keep holding.

The words apply to every turn of the run: typed lines, `/loop` ticks and `/goal` rounds. A delegate
receives the appended words and not the replaced opening. Nothing else reads either: not the aside,
the summariser, the goal judge, the classifier that vets a slot, a processor or the planner of a
manifest run.

The words grant nothing. A write is still put to the person where it would have been, and plan mode
still refuses it. The record of a session stores no system prompt, so a resume without the flag runs
without the words, and the first turn whose system prompt differs from the recorded turns' pays for
a cache miss.

The flags are refused, with the status for an argument (CLI-6), when no words follow them, when the
words are blank, and when they open with `-` and hold no whitespace, since that is the next flag:
taken as the words, `--json` would be removed and the run would answer in the other format. A
sentence that opens with `-` holds a space and is words. If a flag is given twice, the last is used.
They are refused with `--mode manifest`, and with `doctor`, `auth`, `mcp`, `import-leo-creds`,
`import-providers`, `completion` and `shell-init`, which start neither a session nor a task. A refusal writes the
result object of CLI-12 if one was asked for, sends nothing, and says why through the catalogue
([LOCALE-2](localization.md#LOCALE-2)).

**Why.** A person running bravebot from a script or a wrapper has words of their own that the
planner should carry, and with only `AGENTS.md` the words have to be written to a file in the
project first. The words are not labelled, for the reason the user's own message is not: the person
who typed the flag is the person the planner works for ([LABEL-8](labels.md#LABEL-8)).

Replacing only the opening is what keeps the flag from turning the quarantine off. The paragraph
that teaches the planner that a tool's output is data is not in the opening, so a persona cannot
remove it.

**Known costs.** `--append-system-prompt "$(cat notes.md)"` gives the file's bytes the authority of
the person who typed the flag, whatever the file holds and whoever wrote it. Content that should not
have that authority is passed as piped input (CLI-3), which is quarantined. There is no settings
key, no slash command that takes words (`/style` picks a built-in one, [INSTR-12](instructions.md#INSTR-12)), no tool and no `--system-prompt-file`, so words a checkout always wants are
written in its `AGENTS.md`. `auth` is refused as well as the commands that read no words at all,
because no part of `auth` would use them.

`verified-by: bravebot_cli::main::the_system_prompt_flags_are_taken_out_with_the_words_they_gave`
`verified-by: bravebot_cli::main::the_last_system_prompt_named_is_the_one_used`
`verified-by: bravebot_cli::main::a_system_prompt_flag_with_no_words_is_refused`
`verified-by: bravebot_cli::main::words_that_open_with_a_dash_are_taken_when_they_are_a_sentence`
`verified-by: bravebot_cli::main::the_system_prompt_flags_are_refused_where_nothing_would_use_them`
`verified-by: bravebot_cli::running::a_run_given_system_prompts_sends_them_and_keeps_the_rest_of_the_system_prompt`
`verified-by: bravebot_cli::running::a_system_prompt_flag_with_no_words_exits_with_the_argument_status`
`verified-by: bravebot_cli::running::a_manifest_run_is_refused_the_system_prompt_flags`
`verified-by: bravebot_cli::running::a_command_that_runs_no_turn_is_refused_the_system_prompt_flags`
`verified-by: bravebot_agent::turn::a_replaced_opening_takes_the_place_of_the_opening_alone`
`verified-by: bravebot_agent::turn::words_that_allow_writes_allow_none`
`verified-by: bravebot_tui::app::a_session_keeps_the_system_prompt_words_it_started_with_through_every_turn`

<a id="CLI-20"></a>
### CLI-20: `completion <shell>` prints a completion script and does nothing else

`bravebot completion bash`, `zsh` or `fish` writes a script to stdout that completes the
subcommands, the words after `auth`, `mcp` and `completion`, and every flag in the usage table.
Anything else completes as a file path. The script is the whole of stdout and stderr is empty:
CLI-5 says stdout carries the reply, and for this command the script is the reply.

The command makes no request and reads and writes nothing under `~/.bravebot`. The scripts are
built from fixed lists in the program, so the output is the same wherever the command runs, and the
script completes names bravebot defines and never a value read from a directory: no session id, no
definition name, no model name. A command line with no shell, a shell this build has no script for
or any argument after the shell is refused with the status for an argument (CLI-6), prints nothing
on stdout and says why through the catalogue ([LOCALE-2](localization.md#LOCALE-2)). `--agent`, `--system-prompt` and `--append-system-prompt` are refused with `completion`, as
with the other commands that start neither a session nor a task.

**Why.** A person who cannot remember a flag's spelling opens `--help` to find it, and a shell can
offer it on Tab. A script that read the state directory would start a process that reads private
files on every keypress, and a completed session id or definition name would put text from a
workspace into the person's command line, so the script completes only what the program itself
names.

**Known costs.** The lists are written out beside the usage table rather than generated from it,
so adding a flag takes two edits. The test that reads `--help` and compares it with each script is
what fails when they differ. Session ids are not completed.

`verified-by: bravebot_cli::running::a_completion_script_names_every_command_and_flag_the_usage_table_lists`
`verified-by: bravebot_cli::running::a_completion_script_reads_and_writes_nothing_under_the_home`
`verified-by: bravebot_cli::running::a_completion_with_no_script_to_print_is_refused_with_the_argument_status`
`verified-by: bravebot_cli::running::the_bash_script_completes_commands_subcommands_and_flags_by_position`
`verified-by: bravebot_cli::main::a_definition_is_refused_where_nothing_would_work_under_it`
`verified-by: bravebot_cli::main::the_system_prompt_flags_are_refused_where_nothing_would_use_them`

<a id="CLI-21"></a>
### CLI-21: `--advisor <name>` offers the planner a second model to consult

`--advisor <name>` names the model the `advisor` tool asks for one run, in the words `--model`
takes, tier words included. Without it the planner is offered the model the `advisorModel` setting
names, if any ([ADVISOR-8](tools/advisor.md#ADVISOR-8)), and otherwise no such tool. The flag
outranks the setting. The flag is refused
before the run starts when it has no name after it, when the name is blank, when the machine-level
settings refuse the model, when nothing is configured to serve it, and with `--mode manifest`,
which has no planner to ask. What the tool then does is [tools/advisor.md](tools/advisor.md).

**Why.** A blank name is refused for the reason a blank `--model` is: a script that computed an
empty variable asked for an advisor, and running without one would not say so. The refusal for a
model the machine refuses is made before the first round so that a run never starts with an advisor
its first question could not reach.

`verified-by: bravebot_cli::main::an_advisor_flag_names_the_model_the_planner_may_consult`
`verified-by: bravebot_cli::main::a_blank_advisor_is_refused_rather_than_read_as_no_choice`
`verified-by: bravebot_cli::main::an_advisor_the_managed_settings_refuse_is_refused_before_the_run`
`verified-by: bravebot_cli::running::an_advisor_nothing_serves_is_refused_before_the_run`
`verified-by: bravebot_cli::running::an_advisor_is_refused_with_a_manifest_run`

<a id="CLI-22"></a>
### CLI-22: `--safe` starts a run that loads none of a person's own customizations

`--safe` starts a session, a resumed session, a session in lines or a one-shot run that does not read
the person's hooks, skills, delegate definitions, MCP server requests or `AGENTS.md`, in the
person's own directory or in the project. The program's own skills and delegate kinds stay. What
the command line itself names stays too: the model, `--system-prompt`, `--append-system-prompt` and
`--settings`. Sign-in, the model, permission rules, the trust map and credential protection apply
as they do without the flag, so the flag only removes. Once, before the first turn, it says what was
not loaded.

It is taken out of the line wherever it stands, as `--incognito` is, and is not read as part of a
task. It is refused with `--bg`, which starts the session in another process that would not carry it.

**Why.** When a session misbehaves, the first question is whether the person's own configuration is
the cause, and the answer should not take editing five files and putting them back. A mode that
loaded fewer things but also loosened a rule would be a second way to widen a session, so none of
what it leaves out is permission: nothing is added to what the planner may do.

`verified-by: bravebot_cli::running::a_safe_run_loads_none_of_the_customizations_a_plain_one_loads`
`verified-by: bravebot_cli::running::a_run_without_the_safe_flag_says_nothing_of_safe_mode`
`verified-by: bravebot_cli::running::a_safe_session_sends_no_agents_file_from_its_checkout_a_plain_one_sends`
`verified-by: bravebot_cli::main::the_safe_flag_is_taken_out_wherever_it_appears`

<a id="CLI-23"></a>
### CLI-23: `auth status` says whether a sign-in is usable, and its exit status says it too

`bravebot auth status [leo|bedrock|gateway [id]]` reads what a turn reads, writes nothing, and
prints one line per sign-in: `leo`, `bedrock` for the default AWS profile or `bedrock <profile>`,
and `gateway <id>`. Each line is one of:

- **signed in**, with a detail. For Leo that is the line `doctor` prints, the environment and the
  count of credentials unspent. For an AWS account it is that the session gives credentials, asked
  of the AWS CLI without starting a sign-in. For a gateway it is where a key is found, never what it
  is.
- **not signed in**, with the way to sign in: nothing is imported, the AWS session has lapsed, or
  a gateway has no key.
- **unusable**, with the reason and remedy, where signing in again does not fix it: a stored
  batch that cannot be read or was written by another version, or one imported for an environment
  the premium endpoint does not accept, in the words a turn uses
  ([PREM-8](premium-credentials.md#PREM-8)); the AWS CLI missing or the profile unknown; a file of
  gateway keys that cannot be read.

A gateway whose block names nowhere for a key to live needs none and is signed in. Leo needs the
configuration only to know the endpoint, so nothing imported is reported without it. A configuration
that cannot be read is reported as unusable on the lines it blocks.

With no way named, all three are asked about. The command exits 0 if any sign-in is usable, since
nobody holds every way and a script asking whether bravebot can run is asking about the set. With a
way named, it exits 0 only if every sign-in under it is usable. Otherwise it exits with the
configuration status where a configuration could not be read, and the failure status
([CLI-6](#CLI-6)) where not, after the lines, saying which. `import` is refused with the argument
status, since it keeps no sign-in. So are an unknown way, an option, and a word after the way, or
after the id. An id no block has is refused naming the ids configured and not the word given
([CRED-24](credential-protection.md#CRED-24)); with no gateway configured it is the configuration
status.

It is allowed in an incognito session, since it writes nothing.

No line carries a credential, a token, an account or order id, or content from the workspace: only
counts, ids the person wrote in settings, and fixed sentences
([AGENTS.md](../../agents/AGENTS.md)).

**Why.** `doctor` runs every check and reports a subscription inside a longer report, so it gives a
script, or a person confirming that an import worked, no short answer and no exit status that means
signed in. Nothing imported is reported as not signed in, where a turn says nothing about it
([PREM-8](premium-credentials.md#PREM-8)), because here the person asked. Any-of for the set and
all-of for a named way is so that the same command serves a script that wants to know whether to
start and one that wants to know whether a particular sign-in worked.

`verified-by: bravebot_cli::running::auth_status_leo_with_nothing_imported_is_not_signed_in_and_exits_nonzero`
`verified-by: bravebot_cli::running::auth_status_leo_with_a_usable_batch_reports_counts_only`
`verified-by: bravebot_cli::running::auth_status_leo_for_another_environment_prints_the_remedy_a_turn_would`
`verified-by: bravebot_cli::running::auth_status_leo_with_an_unreadable_batch_is_unusable_with_the_stores_remedy`
`verified-by: bravebot_cli::running::auth_status_gateway_says_where_a_key_is_found_and_never_what_it_is`
`verified-by: bravebot_cli::running::auth_status_gateway_with_an_unreadable_file_of_keys_is_unusable`
`verified-by: bravebot_cli::running::auth_status_gateway_naming_no_credential_is_signed_in`
`verified-by: bravebot_cli::running::auth_status_gateway_refuses_an_id_that_names_none_without_repeating_it`
`verified-by: bravebot_cli::running::auth_status_with_no_way_named_succeeds_when_any_sign_in_is_usable`
`verified-by: bravebot_cli::running::auth_status_in_an_unconfigured_build_is_a_configuration_failure`
`verified-by: bravebot_cli::running::auth_status_refuses_what_asks_about_no_sign_in`
`verified-by: bravebot_cli::running::auth_status_bedrock_asks_the_aws_cli_without_signing_in`
`verified-by: bravebot_cli::auth::the_verdict_asks_all_of_a_named_way_and_any_of_the_rest`
`verified-by: bravebot_cli::auth::a_lapsed_aws_session_is_not_signed_in_and_a_missing_cli_is_unusable`
`verified-by: bravebot_cli::auth::nothing_imported_is_told_from_a_store_that_could_not_be_read`

<a id="CLI-24"></a>
### CLI-24: `--json-stream` writes one event per line as the run goes, then the result object

A one-shot run given `--json-stream` is a `--json` run (CLI-12) that also writes an object, on one
line, to stdout as each of these happens, in the order they happen:

- a tool call finishes: `event` is `call`, with `tool`, `target` and `refused` as CLI-12 gives them
  for each entry of `calls`;
- a gate refuses something: `event` is `refusal`, with `gate`, `principle` and `reason` as CLI-12
  gives them for each entry of `refusals`;
- the run's cumulative token usage changes, which is when a model request finishes: `event` is
  `usage`, with `tokens` holding the counts CLI-12 gives under that name. A report that leaves the
  counts as they were is not an event.

The last line is the result object of CLI-12, byte for byte what `--json` writes for the same run.
It has no `event` field, so a caller tells it from an event by that. Every event carries the
`schema` number of CLI-12 and its add-only rule: within one number a field may be added to an event,
and an event kind may be added, and neither is removed, renamed or given a different meaning.

An event holds only fields the result object already holds, so none carries content the driver has
not already released to that object: a tool is named as the driver matched it, and what a call acted
on is the name it was given. A run that stops before any of those happens writes the result object
alone. Stdout holds nothing but these lines, and progress, the message and the trail stay on stderr
as CLI-5 and CLI-12 say. A failed write to stdout is dropped, so a caller that stopped reading does
not stop the run.

**Why.** The result object exists only when the run ends, so a CI wrapper or an editor cannot show
progress, cannot tell a hung run from a slow one, and learns nothing of a run that is killed. The
events are the same facts the object lists, written when they are known, so a caller that follows
the stream and a caller that waits for the last line read the same fields.

A separate flag rather than a change to `--json`, because a caller written against one object per run
would otherwise receive several lines it never expected.

`verified-by: bravebot_cli::running::a_stream_writes_each_event_as_it_happens_and_ends_on_the_result_object`
`verified-by: bravebot_cli::running::a_streamed_run_that_stops_before_the_turn_writes_only_the_result_object`
`verified-by: bravebot_cli::json::an_event_carries_the_fields_the_result_object_carries_for_the_same_thing`
`verified-by: bravebot_cli::json::a_refusal_is_streamed_when_the_gate_takes_it`
`verified-by: bravebot_cli::json::a_run_without_a_stream_writes_no_events`

<a id="CLI-25"></a>
### CLI-25: a one-shot run is written down, and `--resume` and `--continue` carry one on with a task

A one-shot run in the default turn mode writes a session record when its turn ends, as an ordinary
session does ([SESSION-1](sessions.md#SESSION-1)), unless `--incognito` is given
([INCOG-3](incognito.md#INCOG-3)). A run that failed writes none. The result object of CLI-12
carries the record's id in `session`, and carries `null` where the run wrote none.

`--resume <id>` and `--continue` given with a task, in either order (`-p "task" --resume <id>` and
`--resume <id> -p "task"`), run the task as one more turn over that record's conversation and write
the record back with the turn added. Everything else in the record stays as it was read. The run is
a one-shot run in every other respect, so the refusals of CLI-1 hold and nothing is asked. What the
session's person vouched for in the trust map comes back, as it does in a session. The commands
they vouched for do not, and the record keeps them for the session that is next resumed
interactively.

A resumed run shows the planner what a resumed session shows it ([SESSION-2](sessions.md#SESSION-2)):
quarantined content stays a reference, and the references are named as no longer naming anything.
A turn after piped input (CLI-3) is therefore given no byte the first turn was not given.

The flags are refused with the status for an argument (CLI-6), before anything is sent, when no id
follows `--resume`, when the id or the directory's latest session names no record, when the record
is a manifest run ([SESSION-10](sessions.md#SESSION-10)) or a session a background process is
running ([BG-9](background-sessions.md#BG-9)), and with `--mode manifest`. A continued run works
under the definition the record names, and a `--agent` given with the flag names one in place of it
(CLI-17). The result object names the definition in `agent`, and the record names it for the next
run. `--resume` and `--continue` with no task open the session
interactively, and a session in lines writes no record (CLI-14).

**Why.** A script chaining a review, a fix and a summary has to restate everything each time while a
one-shot run starts empty and leaves nothing to start from. Reading the id out of the result object
is the only way a script learns which session to name, so a run that wrote a record and did not say
so would be no better than one that wrote none.

The conversation comes back through the same restore an interactive resume uses, so a rule about
what a planner may be shown is held in one place and a continued one-shot run cannot be shown more
than an interactive one would.

`--continue` and a failed run: the record of a run that failed is not written, so a continuation
after a failure carries on the last turn that completed. What the failed turn did to the working
directory is not in any record, and the result object of a failed run carries `null` for `session`.

Nothing locks a record. Two processes continuing one id, or a one-shot run continuing a session an
interactive process is writing, each write the record they read with their own turn added, and the
later write wins. An interactive resume has the same property, and
[background-sessions.md](background-sessions.md) refuses it only for a background session, where the
process that holds the record is known.

`verified-by: bravebot_cli::running::a_one_shot_run_is_written_down_and_names_its_session`
`verified-by: bravebot_cli::running::a_task_carries_on_the_session_it_names_and_not_the_newest`
`verified-by: bravebot_cli::running::a_task_continuing_the_latest_session_adds_a_turn_to_its_record`
`verified-by: bravebot_cli::running::a_continued_run_is_never_shown_the_bytes_an_earlier_pipe_carried`
`verified-by: bravebot_cli::running::an_incognito_run_is_not_recorded_and_cannot_be_continued`
`verified-by: bravebot_cli::running::a_task_that_cannot_carry_on_a_session_is_refused_before_anything_is_sent`
`verified-by: bravebot_cli::running::a_task_naming_a_manifest_record_is_refused_before_anything_is_sent`
`verified-by: bravebot_cli::running::a_record_a_running_session_holds_is_not_resumed`
`verified-by: bravebot_cli::main::a_resume_with_a_task_after_it_runs_the_task_and_one_without_opens_the_session`
`verified-by: bravebot_cli::main::a_task_may_name_the_session_it_carries_on_before_or_after_it`
`verified-by: bravebot_cli::main::a_resume_naming_no_session_is_refused`
`verified-by: bravebot_session::sessions::continuing_a_record_adds_one_turn_and_leaves_what_it_held_as_read`
`verified-by: bravebot_session::sessions::continuing_a_record_with_no_history_gives_it_none`
`verified-by: bravebot_session::sessions::a_completed_turn_starts_over_when_the_conversation_got_shorter`
