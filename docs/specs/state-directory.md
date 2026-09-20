---
id: STATE
title: The state directory
status: normative
governs:
  - crates/agent/src/home.rs
  - crates/agent/src/remembered.rs
  - crates/config/src/settings.rs
  - crates/session/src/store.rs
  - crates/tui/src/update.rs
  - crates/session/src/sessions.rs
  - crates/skus/src/store.rs
  - crates/lsp/src/server.rs
  - install.sh
guards:
  - symbol: home::create_directory
  - symbol: home::write_file
  - symbol: home::append_to_file
---

## Scope

`~/.bravebot`, the directory holding what outlives a session, and who on the machine may read what
is written into it. The prompt history, the model, theme, effort and editing choices, the answer to
the update question, session records, a language server's index of a workspace, skills, standing
instructions, an imported subscription and the command lines somebody asked to be remembered past a
session all live here.

What each of those files means belongs to the spec for that subject:
[sessions.md](sessions.md) for a session record, [tools/run.md](tools/run.md) for the remembered
command lines, [skills.md](skills.md) and [instructions.md](instructions.md) for what is read out of
the directory,
[premium-credentials.md](premium-credentials.md) for the subscription,
[tools/lsp.md](tools/lsp.md) for the index and why sitting here confers nothing on it, and
[incognito.md](incognito.md) for the mode that writes none of it. This file covers the directory
itself.

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

One helper does this for the crates that can share one. The crate that imports a subscription and
the language server client both sit below the crate holding that helper, as
[layering.md](layering.md) records, so each keeps its own copy of the modes rather than inverting a
dependency for four lines. Either can be the first to write, which is the case that made the mode of
a directory holding prompt history a matter of which subsystem ran first. The language server client
narrows the two directories it owns and not the state directory above them, since what that is set
to belongs to whichever subsystem created it.

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
`verified-by: bravebot_lsp::server::the_cache_is_created_reachable_only_by_its_owner`
`verified-by: bravebot_lsp::server::a_cache_left_open_by_an_earlier_run_is_narrowed`
`verified-by: bravebot_lsp::server::narrowing_does_not_follow_a_link_out_of_the_cache`

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

## Known costs

- **Where the platform is given no mode to create with, the files carry whatever the profile
  directory grants them.** STATE-1 is a Unix rule, and the helpers it names compile to a plain
  directory creation and a plain file open elsewhere: on Windows the prompt history, the session
  records, the recorded model and theme and a language server's index of a workspace are left with
  the permissions inherited from the profile directory. A profile on a shared or a synced volume is
  where that is felt, and the prompt history is the file that matters, since it holds every path,
  branch name and pasted fragment somebody has typed. `doctor` reports which of the two a machine has, so
  this is a weakness a user can read rather than one only recorded here. An imported subscription is
  the exception: it is a bearer token, so a platform whose mode cannot be set has it refused rather
  than written, which [premium-credentials.md](premium-credentials.md) records.
- **A file this program only reads keeps whatever mode it arrived with.** STATE-1 reaches a file as
  something here writes it, and `settings.json`, the standing instructions and the skills are put in
  the directory by the user rather than written by this program, so one placed there at the umask
  stays there. The 0700 on the directory is what covers them: another account cannot open a file
  inside a directory it cannot traverse. What the file modes add is a second answer for the case
  where the directory's own mode is wrong, which is the case a machine that has run an older build
  is in until the first write narrows it.
