---
id: REFER
title: Reference directories
status: normative
governs:
  - crates/config/src/settings.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/preamble.rs
  - crates/agent/src/skills.rs
  - crates/ui-bridge/src/bridge.rs
  - crates/cli/src/main.rs
documented-by: none (proposed with a first slice; the site page follows when the repository half is built)
---

## Scope

A directory outside the working one that a person keeps for the planner to consult: a library's
source, a sibling repository, a folder of notes. This file covers how one is declared, what
declaring it grants, and what the planner is told about it.

Opening a directory by name for one session is [trust-map.md](trust-map.md). A
reference is the same reach, declared once in the person's own settings, with a name and a
description the planner is shown.

## Clauses

<a id="REFER-1"></a>
### REFER-1: an entry names a directory, and one that cannot be used is named and skipped

The `references` block in a settings file maps an alias to a directory. An entry is a string, which
is the path, or an object with a `path` and an optional `description`. A path is absolute or starts
with `~`. An alias is not empty and holds no `/`, whitespace, backtick or comma, so that it can be
written after an `@` later without ambiguity. An entry with a bad alias, with no path, or with a
`repository` and no `path` is not used and is reported by its alias. The entries around it still
apply. A later layer's block replaces an earlier one's.

**Why.** One mistyped entry must not remove the rest of a person's references, and an entry that
silently does nothing is the one a person spends an afternoon looking for.

`verified-by: bravebot_config::settings::the_references_block_reads_each_entry_and_names_the_ones_it_cannot_use`

<a id="REFER-2"></a>
### REFER-2: only the person's own settings declare a reference

`references` is read from `~/.bravebot/settings.json` and from a file `--settings` names that sits
outside the workspace. A checkout's `.bravebot/settings.json` and `settings.local.json` are not
read for it, and each file that wrote the block is reported by `doctor` and by the desktop
bridge's list of ignored keys.

**Why.** An entry makes a directory reachable and puts its description in front of the planner. A
checkout that could write either would choose both for whoever opened it.

`verified-by: bravebot_config::settings::a_project_layer_cannot_declare_references`

<a id="REFER-3"></a>
### REFER-3: a reference is reachable exactly as a directory opened with `--add-dir` is, and records no trust

When a workspace is built for a turn, every usable entry is opened the way `--add-dir` opens a
directory: its files are reachable by absolute path and nothing is recorded in the trust map. Files
there are read on the footing of any file the person has not vouched for. A directory that cannot
be opened, because it is missing, is not a directory, lies inside the working directory or is
refused by `permissions.readsStayInWorkspace`, is told to the person with its alias, is not offered
to the planner, and does not stop the entries after it from opening. The terminal, the one-shot
command line and the desktop bridge each build the workspace this way.

**Why.** Declaring a directory in a settings file is the person naming it, as `--add-dir` is. It is
not a statement that they vouch for its contents, so the trust map is left alone and `/add-dir`
stays the way to say that.

`verified-by: bravebot_agent::preamble::a_reference_is_reachable_and_nothing_else_is_granted`
`verified-by: bravebot_agent::preamble::a_reference_that_cannot_open_is_a_notice_and_not_a_line`
`verified-by: bravebot_agent::workspace::a_tilde_reference_under_a_home_that_is_not_text_is_not_opened_by_its_lookalike`
`verified-by: bravebot_ui_bridge::workspace::a_turn_opens_the_references_the_home_layer_named_and_not_a_projects`
`verified-by: bravebot_cli::running::a_run_tells_the_planner_about_the_references_only_the_person_declared`
`verified-by: by-construction (the terminal and the one-shot command line both build their workspace in cli's current_workspace, which calls with_references)`

<a id="REFER-4"></a>
### REFER-4: the planner is told each open reference's alias, directory and description

The system prompt lists every reference that is open, with its directory and the description the
person wrote, as the person's own words. A reference closed since, with `/add-dir close`, `/clear`
or a `/cd` that overlapped it, is no longer listed. Nothing a reference holds, an `AGENTS.md`
included, is read into the prompt.

**Why.** The description is what lets the planner choose to look there. The list is the person's
text and is trusted as the rest of their settings are. The contents are files nobody vouched for
and reach the planner only through the file tools.

`verified-by: bravebot_agent::preamble::a_reference_is_listed_with_its_directory_and_description`
`verified-by: bravebot_agent::preamble::a_closed_reference_is_no_longer_offered`

<a id="REFER-5"></a>
### REFER-5: a `repository` entry is fetched once, with the person's yes, into `~/.bravebot/references`

Proposed, not built. An entry with a `repository` and an optional `branch` is cloned shallow, once,
by a fetch the person approves through the prompt that names the host ([network-egress.md](network-egress.md)).
What it holds is quarantined and read only through a processor. It is refreshed only by
`/references refresh`, never on its own.

`verified-by: none`

<a id="REFER-6"></a>
### REFER-6: `@alias/path` completes in the box and vouches for nothing more than naming a file does

Proposed, not built. The alias list is offered after `@`, and a path under one is named under the
rules of [naming-files.md](naming-files.md).

`verified-by: none`
