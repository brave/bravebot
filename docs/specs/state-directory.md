---
id: STATE
title: The state directory
status: normative
governs:
  - crates/agent/src/home.rs
  - crates/agent/src/granted.rs
  - crates/agent/src/remembered.rs
  - crates/agent/src/reach.rs
  - crates/agent/src/trusted.rs
  - crates/config/src/settings.rs
  - crates/session/src/store.rs
  - crates/tui/src/update.rs
  - crates/session/src/sessions.rs
  - crates/session/src/retention.rs
  - crates/agent/src/findings.rs
  - crates/skus/src/store.rs
  - crates/diag/src/lib.rs
  - crates/lsp/src/server.rs
  - crates/ui-bridge/src/store.rs
  - install.sh
guards:
  - symbol: home::create_directory
  - symbol: home::write_file
  - symbol: home::append_to_file
documented-by: docs/website/docs/customize/configuration.md
---

## Scope

`~/.bravebot`, the directory holding what outlives a session, and who on the machine may read what
is written into it. The prompt history, the model, theme, effort, language and editing choices, the answer to
the update question, the answer to `/caffeinate`'s explanation, session records, a language server's index of a workspace, skills, standing
instructions, an imported subscription, the command lines somebody asked to be remembered past a
session, the reach somebody remembered for a command, the command lines a shell hook recorded for the terminal still open ([shell-integration.md](shell-integration.md)), the permission rules somebody granted a checkout and the answers to the startup question
somebody asked to be kept all live here, and so does the record of the definitions' memories a
session left untrusted, `untrusted/` ([MEMORY-5](definition-memory.md#MEMORY-5)).
The diagnostic logs, `logs/`, are written there too ([diagnostic-log.md](diagnostic-log.md)). `bravebot bug-report` writes into the working directory and adds nothing here.
[definition-memory.md](definition-memory.md) proposes one more, which nothing yet builds: the
definition a desktop bot is made as. [checkouts.md](checkouts.md) proposes another, which nothing
yet builds either: the checkouts delegates are given, `checkouts/`, which hold a project's files
written readable by their owner alone and are read through the trust map.
[background-sessions.md](background-sessions.md) proposes a third, which nothing yet builds: the
roster of sessions running in the background, `jobs/` ([BG-4](background-sessions.md#BG-4)).

What each of those files means belongs to the spec for that subject:
[sessions.md](sessions.md) for a session record, [tools/run.md](tools/run.md) for the remembered
command lines, [sandboxing.md](sandboxing.md#SANDBOX-23) for the remembered reach,
[permissions.md](permissions.md) for the granted rules,
[trust-map.md](trust-map.md) for the kept answers to the startup question,
[skills.md](skills.md) and [instructions.md](instructions.md) for what is read out of
the directory,
[premium-credentials.md](premium-credentials.md) for the subscription,
[tools/lsp.md](tools/lsp.md) for the index and why sitting here confers nothing on it, and
[incognito.md](incognito.md) for the mode that writes none of it. This file covers the directory
itself.

The programs a `run` starts are refused the whole directory except `sessions/`, which they read and
never write ([SANDBOX-12](sandboxing.md#SANDBOX-12)). A file added to the directory later is refused
without a change to any list.

Three of those kinds of file are bounded by age: session records, a language server's indexes and the
files of what a scan found. [STATE-4](#STATE-4) says which are removed, when, and which are left
whatever their age.

## Clauses

<a id="STATE-1"></a>
### STATE-1: on Unix, the state directory and everything written into it is readable only by the user

On Unix, a directory under `~/.bravebot` is created with mode 0700 and a file written into one
with mode 0600, whichever subsystem is doing the writing. A file's mode is asked for as it is
created rather than applied once it holds anything, and a directory or file that is already there
is narrowed as it is written rather than left as it was found. Narrowing walks up from what was
written as far as the state directory and stops there.

**Why.** The history file is every prompt anybody has typed into this program: the paths they were
working on, the branch names, and whatever they pasted into one. At the process umask that is
world-readable, which on a shared build host or a multi-user machine means readable by every local
account. The other files say less on their own but sit in the same directory and are written by the
same code, and a rule covering some of the files in a directory is one nobody could hold a diff
against.

Narrowing what is already there rather than only what is created new is what makes this reach a
machine that has run an older build, which is every machine that has the history worth protecting.
Creating a directory that exists succeeds without touching its mode, so a fix that only set modes
on creation would leave exactly those machines as they were.

Stopping at the state directory bounds it in the other direction. Whose home this is, and what else
is kept in it, is the user's own business, and a program that narrowed directories it was never
asked about would be making decisions outside anything it was given. A link is stepped over rather
than followed, for the same reason: the bound is a comparison of paths, so it says where a name
sits and nothing about where it leads, and somebody who keeps their sessions on a synced volume and
links the directory into place has put the target outside what this was given.

One helper does this for the crates that can share one. The crate that imports a subscription, the
language server client and the diagnostic log all sit below the crate holding that helper, as
[layering.md](layering.md) records, so each keeps its own copy of the modes rather than inverting a
dependency for four lines. Any of them can be the first to write, which is the case that made the
mode of a directory holding prompt history a matter of which subsystem ran first. The language
server client narrows the two directories it owns and not the state directory above them, since
what that is set to belongs to whichever subsystem created it. The diagnostic log narrows the state
directory as well as `logs/`, because a run that fails before anything else is written has a log
line as its only write, and a state directory an older build left open would stay open after it.

The installer creates the directory too, before the program has run once, so it asks for the same
modes. A directory left at the umask by an install would otherwise stand until the next write went
through the helper.

One write through the file helper lands outside the directory: an export
([SESSION-17](sessions.md#SESSION-17)) writes a transcript into the working directory and asks for
the same mode, because what it holds is what the record holds.

`verified-by: bravebot_agent::home::a_directory_is_created_reachable_only_by_its_owner`
`verified-by: bravebot_agent::home::a_directory_left_open_by_an_older_build_is_narrowed`
`verified-by: bravebot_agent::home::narrowing_stops_at_the_state_directory`
`verified-by: bravebot_agent::home::narrowing_does_not_follow_a_link_out_of_the_state_directory`
`verified-by: bravebot_agent::home::a_file_is_written_readable_only_by_its_owner`
`verified-by: bravebot_agent::home::a_file_left_readable_by_an_older_build_is_narrowed`
`verified-by: bravebot_agent::home::an_appended_file_is_readable_only_by_its_owner`
`verified-by: bravebot_tui::state_directory::the_prompt_history_is_readable_only_by_its_owner`
`verified-by: bravebot_tui::state_directory::a_rewritten_history_is_readable_only_by_its_owner`
`verified-by: bravebot_tui::state_directory::a_recorded_choice_is_readable_only_by_its_owner`
`verified-by: bravebot_tui::state_directory::a_state_directory_an_older_build_left_open_is_narrowed`
`verified-by: bravebot_tui::state_directory::nothing_above_the_state_directory_is_touched`
`verified-by: bravebot_tui::state_directory::writing_a_session_narrows_the_state_directory`
`verified-by: bravebot_skus::store::the_directory_it_is_kept_in_is_not_reachable_by_anyone_else`
`verified-by: bravebot_skus::store::a_file_left_readable_by_something_else_is_narrowed`
`verified-by: bravebot_skus::store::narrowing_does_not_follow_a_linked_state_directory`
`verified-by: bravebot_skus::store::a_claim_is_created_readable_only_by_its_owner`
`verified-by: bravebot_lsp::server::the_cache_is_created_reachable_only_by_its_owner`
`verified-by: bravebot_lsp::server::a_cache_left_open_by_an_earlier_run_is_narrowed`
`verified-by: bravebot_lsp::server::narrowing_does_not_follow_a_link_out_of_the_cache`
`verified-by: bravebot_diag::lib::a_state_directory_left_open_is_narrowed_and_nothing_above_it`
`verified-by: bravebot_diag::lib::narrowing_does_not_follow_a_linked_log_directory`
`verified-by: bravebot_diag::lib::narrowing_does_not_follow_a_linked_state_directory`
`verified-by: bravebot_cli::running::the_hook_steps_over_a_linked_directory_rather_than_narrowing_where_it_leads`

<a id="STATE-2"></a>
### STATE-2: a machine whose platform names no profile directory has no state directory rather than a guessed one

The directory is one fixed name under the profile directory the platform states in the environment.
`HOME` states it on Unix. On Windows `HOME` is read first and `USERPROFILE` second, and the first of
them holding a value answers; a value that is empty states nothing and is passed over. Where none of
them holds one there is no directory at all: nothing is read, nothing is written, and each caller does
without. Every one of them doing without is silent, so `doctor` is where the absence is said out loud,
along with which variables were looked at and what is not kept without a directory to keep it in
([CLI-7](cli.md#CLI-7)). The crates that sit below the one holding the answer resolve the path
themselves, since [layering.md](layering.md) forbids them the dependency, and each spells the same
name, reads the same variables in the same order, and offers the same absence of a fallback.

**Why.** Inventing a location where the platform states none is worse than doing without: it would
mean reading files from somewhere the user never chose, and this is the one directory whose contents
are trusted for being the user's own. A resolver that fell back to a working directory or a temporary
one would put the prompt history somewhere with none of that standing behind it.

A second variable is not that fallback. `USERPROFILE` is the platform stating where the profile is,
exactly as `HOME` does on Unix, so reading it answers the question rather than guessing past it. What
the refusal rules out is a location nobody stated, and a chain ending in a working directory, a
temporary directory or a compiled-in path would be inventing one however many platform variables came
before it. Reading `HOME` first on Windows costs nothing and settles the developer running under a
Unix-like shell there: an environment that sets one has been told where the profile is, and every
other tool started from it reads that.

A rule naming `HOME` alone is a rule under which no stock Windows install has a state directory at
all, since Windows does not set it: no settings of the user's own, no session to resume, no history
and no skills, on a platform this project publishes binaries for.

Independent resolvers are the cost of the layering, and what has to hold across them is the name, the
variables and the refusal to guess, which is what each is pinned on. A resolver that answered a
different name would write a history nothing reads back; one that read a different variable, or read
the same two in the other order, would answer from a directory the next session does not look in; one
that invented a fallback would be the case above, whichever crate it happened in.

`verified-by: bravebot_agent::home::the_home_directory_is_the_one_the_environment_names`
`verified-by: bravebot_agent::home::an_absent_home_is_not_an_error`
`verified-by: bravebot_agent::home::an_empty_home_is_treated_as_no_home_at_all`
`verified-by: bravebot_agent::home::the_profile_directory_answers_where_no_home_is_named`
`verified-by: bravebot_agent::home::a_named_home_answers_before_the_profile_directory`
`verified-by: bravebot_skus::store::no_home_directory_is_reported_rather_than_guessed`
`verified-by: bravebot_skus::store::the_profile_directory_answers_where_no_home_is_named`
`verified-by: bravebot_skus::store::a_named_home_answers_before_the_profile_directory`
`verified-by: bravebot_config::settings::the_state_directory_is_the_home_the_environment_names`
`verified-by: bravebot_config::settings::an_absent_or_empty_home_yields_no_directory_rather_than_a_guess`
`verified-by: bravebot_config::settings::the_profile_directory_answers_where_no_home_is_named`
`verified-by: bravebot_config::settings::a_named_home_answers_before_the_profile_directory`

<a id="STATE-3"></a>
### STATE-3: a surface that only reads this directory does not write into it

A surface above the crates that own what is kept here reads the directory and never writes into it
itself: it may resolve the path, walk it and read a file, and every write goes through the crate
that owns the thing being written. Reading is not restricted, since the modes are what keep the
contents to one account and a reader is already that account.

**Why.** STATE-1 holds because a small number of helpers ask for the modes, and it holds for
whichever subsystem writes first. A second writer opening a file or creating a directory for itself
lands at the process umask instead, and a directory that already exists is not narrowed by being
created again, so one such write decides the mode for every session afterwards. The files most
exposed by that are the ones a second surface writes most: the records hold whole conversations and
the standing permissions a resume restores.

This is a bound on where a write goes rather than a rule about modes, because a surface repeating
the modes would be a third copy of them to keep in step, and one that got them wrong would be wrong
about a directory the other surfaces share.

`verified-by: bravebot_ui_bridge::layering::the_state_directory_is_written_only_through_the_crate_that_owns_it`

<a id="STATE-4"></a>
### STATE-4: a record, an index and a findings file that nothing has written for the retention period are removed when a session starts

Once per process, as a session starts, three kinds of file are removed from the state directory when
nothing has written them for the retention period: a session record, with the trail, the search index
and the temporary file beside it ([sessions.md](sessions.md)); the index a language server keeps of a
workspace ([tools/lsp.md](tools/lsp.md)); and the file recording what a scan found in a workspace
([credential-protection.md](credential-protection.md)).

The period is `retentionDays` in the person's own `settings.json`: a whole number of days, one or
more, and 30 where none is stated. One day is the least it can be, so nothing written in the last day
is removed whatever the setting says. A project or local settings file, and a file the command line
named, cannot state it: the line is read as no statement and `doctor` names the file
([cli.md](cli.md)).

A file's age is its modification time, and no byte it holds decides whether it is kept. A modification
time later than now is not old. A language server's index is aged by a file inside it that each start
of a server on it touches, because the directory's own time does not move while a server writes
deeper inside. An index with no such file was written before it existed: it is given one when first
seen and removed once a whole period has passed from then. A trail or a temporary file whose record
is gone goes by its own age.

The record of the session that is starting, new or resumed, the record of any session a background
process is running, and the index of the workspace the starting session is in are kept at any age.

Nothing is removed when the period cannot be determined: the person's own settings file is there and
cannot be read as a settings document, which an empty file is not, or `retentionDays` in it is not
a whole number above zero. `doctor` names the file. The built-in period is not put in place of one that cannot be read.

Left where they are at any age: the rules a person granted, the answers to the startup question kept
past a session, the memories left untrusted, the acceptances of findings, the prompt history, the
settings, standing instructions, skills and an imported subscription, which a person asked to keep or
wrote themselves. The diagnostic logs are bounded by their own rule
([diagnostic-log.md](diagnostic-log.md)).

Removal stops at the state directory, and the sessions it looks in are the state directory's own: a
root a process was told to keep its records in was named by whoever started it, and is left alone. A
link standing as the directory of sessions, of indexes or of findings, or as one project's
directory, one index or one record, is stepped over and what it leads to is not touched. A project's
directory that the removal leaves empty is removed with its last record, unless a session is
starting in that project. A file that cannot be removed is left, and costs that file only. A file in a
project's directory whose name is not text is not a session's file and is left. An
incognito session removes nothing, and neither does a machine with no state directory.

**Why.** A record holds a whole conversation and the previous contents of files a turn overwrote
inside a directory somebody vouched for, so it holds those for as long as it exists. An index holds
bytes derived from every file in a workspace. Nothing else bounded either, so both lasted as long as
the machine did. A period of 30 days is what Claude Code and Gemini CLI remove a transcript after by
default.

Age is the modification time because the other source is the content, and a record can carry words
from a file or a page. A rule that read a date out of one would let that text keep a file or lose
it. The period is read from the person's own file alone because a project's settings arrive with a
clone: a checkout that stated one day would remove the records of whoever opened it. A period that
cannot be read pauses the sweep rather than falling back to the built-in one, because a removal
cannot be undone and a fallback removes on a figure the person did not choose, where waiting costs
disk. One day is the floor because a period of zero would remove the record of the session somebody
has just left, which is a different request from keeping less. The running session is exempt because
a rewind reads its record, and the next save would write it back, and its workspace's index because a
server is about to start on it. A session a background process is running is left, as removing every
record of a project leaves the one a process is running. A chosen root is not swept because deleting
files by their names and ages is only safe in a directory this program made for them.

The acceptances of findings carry an expiry of their own, and one removed before it reports as open a
finding a person accepted. The trust answers are decisions a person asked to keep past a session, so
age does not end them.

`verified-by: bravebot_config::settings::with_no_retention_stated_the_built_in_period_applies`
`verified-by: bravebot_config::settings::the_home_layer_names_the_period_in_whole_days`
`verified-by: bravebot_config::settings::a_checkout_cannot_choose_when_the_persons_records_are_deleted`
`verified-by: bravebot_config::settings::a_period_that_cannot_be_read_pauses_the_sweep`
`verified-by: bravebot_config::settings::a_home_file_that_cannot_be_read_pauses_the_sweep`
`verified-by: bravebot_config::settings::a_broken_checkout_file_does_not_pause_the_sweep`
`verified-by: bravebot_config::settings::a_link_to_a_settings_file_that_is_not_there_states_no_period`
`verified-by: bravebot_session::retention::a_record_older_than_the_period_is_removed_with_what_is_beside_it_and_a_newer_one_is_kept`
`verified-by: bravebot_session::retention::the_running_sessions_record_is_kept_at_any_age`
`verified-by: bravebot_session::retention::a_records_age_is_when_it_was_written_and_not_what_it_says`
`verified-by: bravebot_session::retention::the_shortest_period_still_keeps_what_was_written_today`
`verified-by: bravebot_session::retention::a_file_stamped_in_the_future_is_kept`
`verified-by: bravebot_session::retention::a_trail_or_temporary_file_with_no_record_goes_by_its_own_age`
`verified-by: bravebot_session::retention::a_name_that_is_not_text_is_left_and_the_file_its_lossy_rendering_names_is_not_removed`
`verified-by: bravebot_session::retention::the_sweep_does_not_follow_a_link_out_of_the_state_directory`
`verified-by: bravebot_session::retention::what_a_person_asked_to_keep_is_left_whatever_its_age`
`verified-by: bravebot_session::retention::a_paused_sweep_removes_nothing`
`verified-by: bravebot_session::retention::a_started_sweep_removes_what_is_old_and_keeps_the_running_session`
`verified-by: bravebot_session::retention::a_session_a_background_process_is_running_is_kept_at_any_age`
`verified-by: bravebot_session::retention::the_directory_of_the_workspace_being_opened_is_not_removed`
`verified-by: bravebot_session::sweeping_a_chosen_root::a_root_the_process_was_told_to_keep_its_records_in_is_not_swept`
`verified-by: bravebot_session::retention::the_index_of_the_workspace_being_opened_is_kept_at_any_age`
`verified-by: bravebot_session::retention::a_project_directory_the_sweep_emptied_is_removed_and_one_holding_anything_is_not`
`verified-by: bravebot_agent::home::only_old_plain_files_are_named_as_unwritten`
`verified-by: bravebot_agent::home::a_link_standing_as_the_directory_names_nothing`
`verified-by: bravebot_agent::home::a_link_standing_as_an_entry_is_not_listed_as_a_directory`
`verified-by: bravebot_lsp::server::an_index_no_server_started_on_for_the_period_is_removed_and_a_newer_one_is_kept`
`verified-by: bravebot_lsp::server::starting_a_server_on_an_index_keeps_it_from_being_removed`
`verified-by: bravebot_lsp::server::an_index_with_no_recorded_use_is_kept_and_removed_a_period_later`
`verified-by: bravebot_lsp::server::the_sweep_does_not_follow_a_link_out_of_the_state_directory`
`verified-by: bravebot_lsp::server::a_file_among_the_indexes_is_left_where_it_is`
`verified-by: bravebot_lsp::server::the_index_of_the_workspace_being_opened_is_kept_at_any_age`
`verified-by: bravebot_agent::findings::a_record_older_than_the_period_is_removed_and_a_newer_one_is_kept`
`verified-by: bravebot_agent::findings::an_acceptance_is_left_where_it_is_whatever_its_age`
`verified-by: bravebot_agent::findings::the_sweep_does_not_follow_a_link_out_of_the_state_directory`
`verified-by: bravebot_agent::findings::a_directory_among_the_records_is_left_where_it_is`
`verified-by: bravebot_cli::doctor::the_retention_section_names_a_paused_sweep_and_a_file_that_was_not_obeyed`
`verified-by: bravebot_cli::running::a_one_shot_run_removes_old_records_and_keeps_the_one_it_continues`
`verified-by: bravebot_cli::running::the_period_in_settings_decides_what_a_one_shot_run_removes`
`verified-by: bravebot_ui_bridge::old_records::a_desktop_front_end_removes_old_records_as_it_starts_and_nothing_a_person_kept`
`verified-by: bravebot_ui_bridge::old_records::a_desktop_front_end_with_an_unreadable_period_removes_nothing`
`verified-by: bravebot_ui_bridge::old_records::an_editor_front_end_removes_old_records_as_it_starts`
`verified-by: bravebot_ui_bridge::host_process::a_host_sweeps_the_state_directory_and_leaves_the_root_it_was_given`

## Known costs

- **A removal cannot be undone, and the built-in period applies to a person who never chose one.** A
  record older than 30 days cannot be resumed, forked or searched, and nothing archives it first. A
  person who wants one kept runs `/export` before it ages out
  ([SESSION-17](sessions.md#SESSION-17)), or states a larger `retentionDays`.
- **A session left open past the period is swept by another process.** The exemption names the
  session and workspace of the process that sweeps and the sessions a background process is running,
  so a session open in a second terminal for longer than the period has its record removed by the
  first, and its next save writes the record back. A record that process saves between the sweep
  reading its age and removing it goes with the rest. A language server running when its index goes
  writes it again, at the cost of the indexing.
- **A desktop or editor front end exempts no session.** It sweeps before any session is open, so a
  record it is asked to load afterwards has been judged by its age alone.
- **Records in a root a process was told to keep them in are never swept.** A host started with a
  root of its own keeps them there until somebody removes them.
- **The sweep runs once per process.** A host kept running for months sweeps when it starts and not
  again, so what ages out after that stays until the next start.
- **The kept answers accumulate.** `granted/`, `trusted/`, `untrusted/` and `findings/accepted/` hold
  what a person asked to keep, so age does not end one, and an acceptance past its expiry is ignored
  and not removed. A machine that has been in many directories holds a file for each.
- **A sweep that is stopped part way is finished by the next one.** The process can end while the sweep
  thread is running, which leaves a record whose trail went first, and still opens, or an index whose
  directory is partly removed, which a server starts on and fills again. A client that loads a record
  as the sweep removes it reads it without its trail.
- **Where the platform is given no mode to create with, the files carry whatever the profile
  directory grants them.** STATE-1 is a Unix rule, and the helpers it names compile to a plain
  directory creation and a plain file open elsewhere: on Windows the prompt history, the session
  records, the recorded model and theme and a language server's index of a workspace are left with
  the permissions inherited from the profile directory. A profile on a shared or a synced volume is
  where that is felt, and the prompt history is the file that matters, since it holds every path,
  branch name and pasted fragment somebody has typed. `doctor` reports which of the two a machine has, so
  this is a weakness a user can read rather than one only recorded here. An imported subscription is
  the exception: it is a bearer token, so it is restricted to one account by whatever the platform
  does have (a mode on Unix, an access-control list on Windows) and refused rather than written where
  it has neither, which [premium-credentials.md](premium-credentials.md) records. That buys the one
  file and not the directory holding it, which is why the sentence above still stands.
- **A file this program only reads keeps whatever mode it arrived with.** STATE-1 reaches a file as
  something here writes it, and `settings.json`, the standing instructions and the skills are put in
  the directory by the user rather than written by this program, so one placed there at the umask
  stays there. The 0700 on the directory is what covers them: another account cannot open a file
  inside a directory it cannot traverse. What the file modes add is a second answer for the case
  where the directory's own mode is wrong, which is the case a machine that has run an older build
  is in until the first write narrows it.
