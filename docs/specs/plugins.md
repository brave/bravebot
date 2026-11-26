---
id: PLUGIN
title: Plugins
status: proposed
governs:
  - crates/config/src/hooks.rs
  - crates/config/src/mcp.rs
  - crates/config/src/plugin.rs
  - crates/agent/src/plugins.rs
  - crates/agent/src/plugin_hooks.rs
  - crates/agent/src/plugin_servers.rs
  - crates/config/src/hooks_approved.rs
  - crates/agent/src/plugin_source.rs
  - crates/agent/src/plugin_git.rs
  - crates/tui/src/plugin_command.rs
  - crates/tui/src/confirm/plugin.rs
  - crates/tui/src/confirm/hook.rs
documented-by:
  - docs/website/docs/customize/plugins.md
---

## Scope

One person-run command that installs a bundle of skills, delegate definitions, hooks and MCP server
declarations from one source, what the person is shown before it does, where each part lands, what a
plugin can never carry, and how an installed plugin is updated, turned off and removed.

What is built is the install of a plugin from a local path or from a git URL and ref with
`/plugin add`, and `/plugin update`, `/plugin list`, `/plugin remove`, `/plugin enable` and
`/plugin disable`, for the skills, definitions, hooks and servers a plugin carries, with the hook
question and the server question of [PLUGIN-7](#PLUGIN-7) and `${PLUGIN_ROOT}`
([PLUGIN-13](#PLUGIN-13)). A clause only partly built has a **Not built** paragraph naming what is
missing. The specs this one changes are named in
[What this changes elsewhere](#what-this-changes-elsewhere). The skills, definitions, instructions,
trust-map, hooks and MCP servers specs were edited when the work they describe landed, and the
programs and import specs are not edited until theirs does.

A **plugin** is a directory with a `plugin.toml` at its root. A **source** is a git URL or a local path
that holds one. The **install** is `/plugin add`, which a person types. There is no marketplace, no
catalog and no index of plugins: a person names the source.

Where a skill, a definition, a hook or a server is read from, and what each is trusted for, is
[skills.md](skills.md), [delegation.md](delegation.md), [hooks.md](hooks.md) and
[mcp-servers.md](mcp-servers.md). What the trust map records is [trust-map.md](trust-map.md), and what a
fetch asks is [tools/fetch-url.md](tools/fetch-url.md) and [network-egress.md](network-egress.md). The
prompts a person is asked are in [prompting.md](prompting.md). The rule that untrusted content never
reaches the driver or the planner is [AGENTS.md](../../agents/AGENTS.md).

## What exists today

A person installs each piece by hand: a skill directory under `~/.bravebot/skills`, a definition at
`~/.bravebot/agents/<name>.md`, entries in `~/.bravebot/hooks.json`, and `bravebot mcp add` for a
server. Each is trusted on its own terms. Nothing names a bundle, a version or an update, and
[import.md](import.md) lists opencode's `plugin` key as not imported.

## Clauses

<a id="PLUGIN-1"></a>
### PLUGIN-1: a plugin is installed only by a command a person types

`/plugin add <source>` is the only way a plugin is installed. `/plugin update`, `/plugin remove`,
`/plugin enable`, `/plugin disable` and `/plugin list` are the others. No tool the planner calls
installs, updates, removes, enables or disables one, no checkout file requests one, and a line
arriving through a quarantined source (a pasted block, a fetched page, a file in a workspace) is not
the typed command even where it reads as one.

**Why.** An install puts programs and instructions into the directory every later session trusts by
provenance. Whether that happens is a decision about where bytes land and what runs afterwards, and
only a person's own keystrokes are a source for it that no content can author.

`verified-by: bravebot_agent::tools::no_tool_offered_installs_or_changes_a_plugin`
`verified-by: bravebot_tui::app::tests::commands_words::the_plugin_command_carries_its_words_unparsed`
`verified-by: bravebot_tui::app::tests::remainder::a_pasted_plugin_line_is_text_in_the_box_and_not_the_command`
`verified-by: bravebot_cli::running::a_one_shot_task_opening_with_slash_plugin_add_installs_nothing`
`verified-by: bravebot_cli::running::a_one_shot_task_opening_with_slash_plugin_update_changes_nothing`
`verified-by: bravebot_tui::plugin_command::the_words_after_the_command_read_as_the_six_forms_and_nothing_else`

<a id="PLUGIN-2"></a>
### PLUGIN-2: `plugin.toml` names its parts, and a part it does not name is not installed

The manifest holds a `name` (letters, digits and hyphens, the directory name under
`~/.bravebot/plugins`), a `version`, a `description`, and optional paths for `skills`, `agents`,
`hooks` and `servers`. `skills` and `agents` are directories, `hooks` is one file and `servers` is one
file. These four are the only kinds of part. Every path is relative to the plugin's root and stays
inside it: an absolute path, a `..` and a link leaving the directory are refused, and the install
stops, naming the manifest key.

A directory the manifest does not name is not scanned for parts. A key this build does not read is
reported and ignored. A manifest with a key that names a permission rule, an allow list, a grant, a
trusted path or a setting is refused whole ([PLUGIN-6](#PLUGIN-6)).

`plugin.toml` is the only manifest read. A directory holding another tool's manifest and no
`plugin.toml` is refused, and the refusal names `plugin.toml` as the missing file.

A `hooks` or `servers` file the manifest names is read from the plugin's directory as data and listed
([PLUGIN-4](#PLUGIN-4)). A named file that is missing refuses the install. One that exists and cannot be
used (a link, not a regular file, over 64 KB, not text, or not in the format of `hooks.json` or
`mcp.json`) is installed with the fault listed, and nothing runs from it.

**Why.** A reader that finds parts by folder name installs whatever the author left lying in the
checkout. A part the person was shown has to be one the manifest named. A second manifest format would
need a translation from another tool's hook moments and server fields into this one's, and the person
would be shown the translation rather than the file.

`verified-by: bravebot_config::plugin::a_manifest_gives_its_name_version_description_and_parts`
`verified-by: bravebot_config::plugin::a_part_the_manifest_does_not_name_is_none`
`verified-by: bravebot_config::plugin::hooks_and_servers_are_named_parts_like_the_others`
`verified-by: bravebot_config::plugin::a_part_file_that_is_a_link_is_not_followed`
`verified-by: bravebot_config::plugin::a_file_that_cannot_be_used_is_a_fault_naming_why`
`verified-by: bravebot_agent::plugins::a_named_part_file_that_is_missing_is_refused_and_one_that_is_unusable_is_a_fault`
`verified-by: bravebot_agent::plugins::a_staged_plugin_reads_its_hooks_and_servers_with_the_path_it_will_have`
`verified-by: bravebot_agent::plugins::the_installed_read_gives_the_list_the_staged_read_gave`
`verified-by: bravebot_config::plugin::an_absolute_part_path_is_refused_naming_the_key`
`verified-by: bravebot_config::plugin::a_parent_part_path_is_refused_naming_the_key`
`verified-by: bravebot_config::plugin::a_part_path_naming_nothing_below_the_root_is_refused`
`verified-by: bravebot_config::plugin::a_name_is_letters_digits_and_hyphens_only`
`verified-by: bravebot_config::plugin::a_key_this_build_does_not_read_is_reported_and_ignored`
`verified-by: bravebot_config::plugin::another_tools_manifest_alone_is_refused_naming_plugin_toml`
`verified-by: bravebot_config::plugin::read_opens_plugin_toml_alone_and_names_nothing_else`
`verified-by: bravebot_config::plugin::a_decision_key_refuses_the_manifest_whole_however_it_is_written`
`verified-by: bravebot_config::plugin::a_part_that_is_a_link_leaving_the_root_is_refused_naming_the_key`
`verified-by: bravebot_agent::plugins::a_link_leaving_the_plugin_is_refused_before_anything_is_copied`

<a id="PLUGIN-3"></a>
### PLUGIN-3: fetching a source is a network effect, and the host is put to the person before it happens

A git URL is cloned shallow into a directory outside `~/.bravebot` and outside every workspace.
The host is shown and asked about through the same prompt a fetch uses, and no request leaves before
the yes. Only `https` and `ssh`-style git URLs and local paths are sources. A redirect to another host
is not followed. A source that is an archive, an npm package or a plain URL is refused.

What a source is is decided from the typed text alone. An `https` URL is a git source when its path
ends in `.git`, and any other `http` or `https` URL is a plain URL. An `ssh://host/path` URL and the
`user@host:path` form are git sources. A `file://` URL, `git://`, `ftp://` and `git+https://` are
refused, and a local directory is typed as a path. A ref starting with `-` or holding a control
character is refused. The host in the question is the one the URL parser reads, with no user
information and no port, and no request is made for a source that is refused.

The clone is made by the `git` program, started the way [SANDBOX-30](sandboxing.md#SANDBOX-30)
requires, with the variables that select code removed from its environment, no submodule fetched and no
hook from the cloned repository run. The program is told on its command line, where no file outranks
it, not to follow a redirect, to use only the `https` and `ssh` protocols and to ignore a hooks
path and a file system monitor. No step runs inside the cloned tree, which holds no `.git`. A `git`
found only in a directory a confined stage may write reads as not installed, and the install is
refused before the host is asked about. A local path is read in place, copied into staging and never
executed there.

The clone is made in a fresh directory, private to the account and named at random, under the system
temporary directory, which is outside `~/.bravebot` and outside every workspace unless a workspace is
opened on the temporary directory itself. It is removed when the install ends, is refused or is
declined. The tree is copied from it to a `.plugin-staging-*` directory in `~/.bravebot` before the
listing is built, so what is listed and installed is the copy. A local path is copied to the same
kind of directory without a clone.

The staged bytes are untrusted content. Nothing in them is read by the driver or the planner before the
person has seen what [PLUGIN-4](#PLUGIN-4) lists, and the listing is built by a reader that parses the
manifest and the parts as data. No part of the staged tree is executed, loaded or followed to decide what
is listed.

**Why.** The download is the first moment untrusted bytes exist, and the person's yes to the host is
the same decision a fetch of a page asks. A staged tree that something already ran would have decided
for the person.

`verified-by: bravebot_agent::plugin_source::an_archive_an_npm_name_and_a_plain_url_are_refused_each_for_its_own_reason`
`verified-by: bravebot_agent::plugin_source::the_host_is_the_one_connected_to_and_an_option_like_authority_is_refused`
`verified-by: bravebot_agent::plugin_source::one_trailing_slash_after_dot_git_is_a_git_source`
`verified-by: bravebot_agent::plugin_source::a_local_path_is_a_source_taken_whole`
`verified-by: bravebot_agent::plugins::typed_text_that_is_not_a_source_asks_and_writes_nothing_and_a_path_still_installs`
`verified-by: bravebot_agent::plugins::no_connection_is_made_to_the_host_before_a_yes_and_a_no_makes_none`
`verified-by: bravebot_agent::plugins::the_host_question_carries_the_parsed_host_and_the_url_and_is_put_once`
`verified-by: bravebot_agent::plugins::no_git_or_nobody_to_ask_puts_no_question`
`verified-by: bravebot_agent::plugins::a_yes_to_the_host_and_to_the_listing_installs_the_commit_the_branch_pointed_to`
`verified-by: bravebot_agent::plugin_git::every_step_pins_the_redirect_hook_fsmonitor_and_protocol_settings_on_its_argv`
`verified-by: bravebot_agent::plugin_git::a_git_found_only_where_a_stage_may_write_reads_as_not_installed`
`verified-by: bravebot_agent::plugin_git::the_installed_tree_holds_no_git_directory_and_a_failed_fetch_leaves_nothing`
`verified-by: bravebot_agent::plugin_git::the_fetch_directory_is_private_and_never_one_that_was_already_there`
`verified-by: bravebot_agent::plugin_git::the_variables_that_name_a_program_for_git_or_ssh_are_removed`
`verified-by: bravebot_agent::plugin_git::this_agents_credentials_and_the_repository_variables_are_not_passed`
`verified-by: bravebot_agent::plugin_git::safety_tests::an_archive_an_npm_name_and_a_plain_url_start_no_program_and_ask_nothing`
`verified-by: bravebot_agent::plugin_git::safety_tests::a_protocol_that_is_not_on_the_list_is_refused_by_git`
`verified-by: bravebot_agent::plugin_git::safety_tests::a_redirect_to_another_host_is_not_followed_and_the_install_is_refused`
`verified-by: bravebot_agent::plugin_git::safety_tests::planted_source_tests::a_submodule_the_repository_names_is_not_fetched`
`verified-by: bravebot_agent::plugin_git::safety_tests::planted_source_tests::no_hook_and_no_file_system_monitor_runs_and_the_clone_holds_no_hook`
`verified-by: bravebot_agent::plugin_git::safety_tests::planted_source_tests::git_is_never_run_inside_the_tree_and_the_tree_holds_no_git_directory`
`verified-by: bravebot_agent::plugin_git::safety_tests::planted_source_tests::nothing_in_the_cloned_tree_runs_before_the_listing_and_a_link_is_not_copied`
`verified-by: bravebot_agent::plugin_git::safety_tests::planted_source_tests::a_cloned_link_in_a_part_leaving_the_plugin_is_refused_and_leaves_nothing`
`verified-by: bravebot_agent::plugin_git::safety_tests::environment_tests::variables_that_select_code_set_in_the_persons_environment_do_not_reach_the_clone`
`verified-by: bravebot_agent::plugin_git::safety_tests::environment_tests::a_template_directory_in_the_persons_environment_is_not_used_by_the_clone`
`verified-by: bravebot_tui::plugin_command::a_git_url_is_put_in_the_fetch_prompt_with_its_host_apart_and_a_no_ends_it`
`verified-by: bravebot_tui::plugin_command::every_permission_mode_still_puts_the_host_question_and_a_lost_terminal_is_a_no`
`verified-by: bravebot_tui::plugin_command::add_refuses_an_archive_an_npm_name_and_a_plain_url_in_words_without_asking`

<a id="PLUGIN-4"></a>
### PLUGIN-4: the install lists every part in full and asks once

Before anything is copied, the person is shown the plugin's name, version, source and resolved commit
([PLUGIN-14](#PLUGIN-14)), and then each part: every skill's name, description and the `model` it
names, every definition's name, kind and the `model` it names, every hook's moment and argv, every
server's transport, argv or url and host, the variables it names and the files it may read. Anything
the part would reach that [network-egress.md](network-egress.md) or [permissions.md](permissions.md)
would otherwise ask about (a host, a command, a path outside the workspace) is on its own row. A skill
or definition that a source of higher rank would shadow ([PLUGIN-12](#PLUGIN-12)) is marked as
shadowed. The text drawn from the plugin is drawn inside the margin untrusted content in a prompt is
drawn inside, and is cut by what a person can read, with the rest of a long part shown on request and
never dropped.

The answer is yes or no. A yes installs every part, and the prompt says plainly what it does not do:
a hook is not made to run and a server is not made reachable by it, which each reach through their own
first-use question ([PLUGIN-7](#PLUGIN-7)). No mode answers this prompt and no allow rule does. Where
nobody can be asked, such as a one-shot run, nothing is installed.

The hook rows give the moment, the tool a `tool-finished` entry names, the file a `file-changed` entry
watches, the timeout, and each word of the argv on a row of its own with `${PLUGIN_ROOT}` already
replaced ([PLUGIN-13](#PLUGIN-13)). The server rows give the alias, the transport, each word of the
argv or the url, the variables named and the stored variables as `NAME=value`, and the files it may
read, and a server entry that cannot be used is listed by alias and marked never started. So is an entry
whose alias is fine alone but too long once the plugin's name and `_` are put in front of it, since a
session cannot reach it under that name. What a
declaration states that reaches outside the plugin is on a row of its own: the program started, the
host of a url and of each allowed host, each file it may read and the directory it runs in. A hooks
file entry the reader passed over is counted in the prompt and not shown, and a hooks or servers file
that cannot be read is said and lists nothing. No row is cut: every argv word is drawn whole.

**Why.** A person cannot vouch for a thing they were not shown, and a prompt listing the plugin's own
summary of itself would let the plugin choose what the person reads. The prompt is built from the
parts, not from a field the manifest wrote.

**Not built:** a row for a path or a host that an argv word names. A hook's or a server's argv is
shown word by word and nothing reads a path or a host out of one word to classify it, so the rows above
are those a declaration states in a field of its own. The resolved commit row is built for a git
source, and a local path has none.

`verified-by: bravebot_tui::confirm::plugin::the_prompt_lists_every_part_and_says_what_a_yes_does_not_do`
`verified-by: bravebot_tui::confirm::plugin::hooks_and_servers_are_listed_in_full_with_the_installed_path_and_their_reach`
`verified-by: bravebot_tui::confirm::plugin::a_server_whose_joined_alias_is_too_long_is_listed_as_never_started`
`verified-by: bravebot_tui::confirm::plugin::an_unreadable_hooks_file_is_said_and_not_listed_as_no_hooks`
`verified-by: bravebot_config::plugin::what_the_hook_reader_passes_over_is_counted`
`verified-by: bravebot_config::plugin::a_server_entry_that_cannot_be_used_is_listed_with_its_problem`
`verified-by: bravebot_tui::confirm::plugin::a_git_source_shows_the_requested_ref_and_the_full_commit`
`verified-by: bravebot_tui::confirm::plugin::text_from_the_plugin_is_drawn_as_untrusted_content`
`verified-by: bravebot_tui::confirm::plugin::a_long_description_is_cut_and_shown_on_request_and_a_yes_waits_for_it`
`verified-by: bravebot_tui::confirm::plugin::a_shadowed_skill_and_definition_are_marked`
`verified-by: bravebot_tui::confirm::plugin::only_a_y_installs`
`verified-by: bravebot_tui::plugin_command::every_permission_mode_still_puts_the_prompt_to_the_person`
`verified-by: bravebot_tui::plugin_command::add_where_nobody_can_be_asked_installs_nothing_and_says_so`
`verified-by: bravebot_agent::plugins::where_nobody_can_be_asked_nothing_is_installed`
`verified-by: bravebot_agent::plugins::only_a_yes_installs_and_the_question_is_put_once`
`verified-by: bravebot_agent::plugin_sources::a_staged_plugin_is_marked_with_the_sources_that_shadow_its_names`

<a id="PLUGIN-5"></a>
### PLUGIN-5: a plugin installs at user scope, into one directory the person's own, and a workspace plugin is inert

The install copies the staged plugin to `~/.bravebot/plugins/<name>/` and records its source, the
requested and resolved ref ([PLUGIN-14](#PLUGIN-14)), whether it is enabled
([PLUGIN-11](#PLUGIN-11)) and the SHA-256 of the copied tree beside it. Reading that directory is
trusted by provenance, as `~/.bravebot/agents` is. A plugin directory found in a workspace, including
one named `.bravebot/plugins`, offers nothing, and is a file read through the trust map like any other
and therefore not trusted until the map says so.

An install whose name is already installed from another source is refused rather than replaced, naming
both sources. An install of the same source is refused too, and the refusal names
`/plugin update <name>` ([PLUGIN-9](#PLUGIN-9)), which is how an installed plugin is replaced. A git
source is the same source when its URL and requested ref are the same, whatever commit the ref resolves
to now.

There is no project scope and no setting that makes a plugin required for a team.

**Why.** The state directory is the one place a clone cannot reach, which is the property every
single-file spec of the declarations depends on. A project scope would put plugin parts where a checkout
authors them.

`verified-by: bravebot_agent::plugins::an_install_puts_the_tree_and_a_record_under_the_plugins_directory`
`verified-by: bravebot_agent::plugins::a_name_installed_from_another_source_is_refused_naming_both_and_changes_nothing`
`verified-by: bravebot_agent::plugins::adding_the_same_source_again_is_refused_and_points_at_update`
`verified-by: bravebot_tui::plugin_command::adding_an_installed_source_points_at_update_and_does_not_say_it_is_not_built`
`verified-by: bravebot_agent::plugins::a_git_record_holds_the_url_ref_and_commit_and_the_same_url_and_ref_is_an_update`
`verified-by: bravebot_agent::plugins::an_old_record_still_reads_and_a_damaged_git_record_does_not`
`verified-by: bravebot_agent::plugin_source::the_same_url_and_ref_is_one_source_whatever_commit_it_resolved_to`
`verified-by: bravebot_agent::plugins::a_name_that_could_leave_the_plugins_directory_is_refused_before_a_path_is_made`
`verified-by: bravebot_agent::plugins::an_install_leaves_the_rules_the_trust_map_and_the_state_beside_it_untouched`
`verified-by: bravebot_agent::plugin_sources::an_enabled_plugin_offers_its_skill_and_definition_and_a_disabled_one_offers_nothing`
`verified-by: bravebot_agent::plugin_sources::a_plugin_directory_in_a_workspace_offers_nothing`

<a id="PLUGIN-6"></a>
### PLUGIN-6: a plugin carries no decision

The manifest declares no permission rule, no allow, ask or deny entry, no grant, no trusted path, no
mode, no model, no endpoint and no setting. A part that is a skill, a definition or a hook is held to
what that part already may say: a definition cannot name a capability set beyond its kind, and a hook
decides nothing. Installing a plugin does not change the permission rules in force, the trust map or
the mode of any session.

A skill or a definition in a plugin may name a `model` as one in `~/.bravebot` may
([SKILL-15](skills.md#SKILL-15), [DELEGATE-22](delegation.md#DELEGATE-22)), and it is read the same
way. The manifest and every other file in the plugin name none. The install shows each such model
([PLUGIN-4](#PLUGIN-4)), so the person's yes to the listing is the endorsement that a file in their own
state directory is for the same key. A plugin directory in a workspace is inert
([PLUGIN-5](#PLUGIN-5)), so no checkout names a model through a plugin.

**Why.** A plugin is outside content a person chose to trust, and the choice was to read what it lists.
A permission rule is a decision taken from something a person wrote, and a plugin is not that person.
A model decides which service receives the conversation ([BACKEND-24](backends.md#BACKEND-24)), so a
plugin may name one only where the listing the person answered showed it.

`verified-by: bravebot_config::plugin::a_manifest_with_a_decision_key_is_refused_by_read_and_nothing_is_made`
`verified-by: bravebot_config::plugin::a_key_that_only_resembles_a_decision_key_is_not_one`
`verified-by: bravebot_config::plugin::the_decision_list_and_the_keys_read_do_not_overlap`
`verified-by: bravebot_config::plugin::the_manifest_has_no_field_that_can_carry_a_decision`
`verified-by: bravebot_config::plugin::a_decision_key_refuses_the_manifest_whole_however_it_is_written`
`verified-by: bravebot_core::delegate::a_definition_can_only_narrow_what_its_kind_holds`
`verified-by: bravebot_agent::plugin_hooks::a_plugin_hook_is_told_the_moment_only_and_its_output_is_not_read`

<a id="PLUGIN-7"></a>
### PLUGIN-7: installing a part does not approve it

A skill from a plugin is trusted for what a skill in `~/.bravebot/skills` is trusted for, and no more.
A definition from a plugin is one `~/.bravebot/agents` would hold. A server from a plugin is reachable
only after the first-use question that names its digest, as a server added by `bravebot mcp add` is,
and the install records no answer to it. Bypassing, a one-shot run and a recorded project answer do
not answer that question for a plugin's server, which differs from a server the person declared, and
only a person's yes does ([SERVERS-27](mcp-servers.md#SERVERS-27)). Installing and enabling a plugin is
the request for its servers, so no `mcp.request` is written.

A hook from a plugin fires only after the person has answered a question naming its moment and its
argv as [PLUGIN-13](#PLUGIN-13) expands it. The question is asked at the first moment the hook would
fire, in a session a person is watching, and at most once in a session. A yes is recorded in
`~/.bravebot/hooks-approved` as a digest of the plugin's name, the hook's moment, its expanded argv and
the plugin's tree digest. A no records nothing, the hook does not fire, and the question is asked again
in a later session. Where nobody can be asked, the hook does not fire. No permission mode, bypassing
included, answers the question. An incognito session asks it and does not write the yes, which lasts
until the session ends. [hooks.md](hooks.md#HOOK-11) says which moments fire a plugin's hook and where
none does.

Before a hook fires or a server starts, the plugin's directory is hashed again, and for a server also
before its question. A tree whose digest is not the one recorded at install stops the hook or the
server without a question, and a notice names the plugin once in the session as a plain sentence.
Every hook of a plugin, and every server, is a question again after an update the person accepted,
whether or not its own declaration changed, because the digest of a hook's approval and of a server's
approval each include the digest of the plugin's tree ([PLUGIN-9](#PLUGIN-9)). The recorded digest is
replaced by the update, so the directory matches its record afterwards and the hook and the server are
not stopped as changed.

**Why.** The install is one yes to a list. Running a program the first time is a decision about that
program, and it is asked about where it runs, in a session the person is watching. A hook is usually an
argv that starts a script in the plugin, so an approval bound to the argv alone would still hold after
the script changed.

`verified-by: bravebot_agent::plugin_hooks::a_plugin_hook_runs_nothing_until_the_question_is_answered_yes`
`verified-by: bravebot_agent::plugin_hooks::a_no_leaves_the_hook_silent_and_is_asked_again_in_a_later_session`
`verified-by: bravebot_agent::plugin_hooks::a_recorded_yes_is_not_asked_again_in_a_later_session`
`verified-by: bravebot_agent::plugin_hooks::where_nobody_can_be_asked_no_plugin_hook_fires`
`verified-by: bravebot_agent::plugin_hooks::a_script_edited_after_a_yes_stops_the_hook_and_names_the_plugin_once`
`verified-by: bravebot_agent::plugin_hooks::a_tree_that_differs_is_not_asked_about`
`verified-by: bravebot_agent::permission_mode::no_mode_answers_the_question_about_a_plugins_hook`
`verified-by: bravebot_tui::confirm::hook::a_yes_that_will_not_be_written_says_so`
`verified-by: bravebot_agent::plugins::an_install_records_no_answer_for_a_hook_or_a_server`
`verified-by: bravebot_agent::plugins::the_installed_tree_is_hashed_again_and_compared_with_the_record`
`verified-by: bravebot_agent::servers::plugin_tests::a_plugins_server_is_not_reachable_before_its_question_and_a_no_records_nothing`
`verified-by: bravebot_agent::servers::plugin_tests::nothing_but_a_yes_to_that_server_approves_it`
`verified-by: bravebot_agent::servers::plugin_tests::a_plugin_whose_files_changed_starts_no_server_before_or_after_its_question`
`verified-by: bravebot_agent::mcp::a_plugins_server_is_stopped_by_a_changed_tree_and_the_plugin_is_named_once`
`verified-by: bravebot_agent::plugin_hooks::after_an_update_every_hook_of_the_plugin_is_asked_about_again`
`verified-by: bravebot_agent::servers::plugin_tests::a_server_of_an_updated_plugin_is_asked_about_again_though_its_declaration_is_the_same`
`verified-by: bravebot_agent::plugins::a_yes_replaces_the_plugin_keeps_it_disabled_and_withdraws_only_its_own_answers`

<a id="PLUGIN-8"></a>
### PLUGIN-8: withdrawn, there is no marketplace

Replaced by [PLUGIN-14](#PLUGIN-14) and [PLUGIN-5](#PLUGIN-5). No index of plugins exists and none is
registered. A source is a git URL or a local path the person names, and the pin that an index entry
would carry is the resolved commit and the tree digest those two clauses record.

<a id="PLUGIN-9"></a>
### PLUGIN-9: update, remove and list work from the recorded source

`/plugin update <name>` fetches the recorded source again at the recorded ref
([PLUGIN-3](#PLUGIN-3), [PLUGIN-14](#PLUGIN-14)) and compares the tree's digest with the recorded one.
A git source goes through the host question and the staging an install uses, so no request leaves
before a yes to the host and no mode or rule answers it. A local path is staged again from the
recorded path, with no host question since nothing leaves the machine. A source recorded at a full
commit has nothing to update to, and that is said before anything is fetched or asked. A source that
now holds a plugin of another name is refused with nothing changed. A plugin whose record cannot be
read, or whose recorded git source no longer reads as one, is refused with nothing changed, and is
removed and added again.

When the digest is the same, nothing changes and nothing is asked: the record, the directory and the
answers are untouched, including when the ref now resolves to a newer commit, which is not recorded and
is said in words. When the digest differs, the difference is put to the person the way
[PLUGIN-4](#PLUGIN-4) puts a first install: the name, the old and new version, the source, the old and
new resolved commit, and each skill, definition, hook and server added, removed or changed, in full
behind the same margin, with a list of the files that differ so a changed script is visible where the
parts read the same. The installed copy is read as data for that listing, and an installed copy that
cannot be read is said and is not shown as empty. The answer is yes or no. No mode and no allow rule
answers it, and where nobody can be asked, such as a one-shot run, nothing is read or updated.

A part whose name another installed plugin already offers ([PLUGIN-12](#PLUGIN-12)) refuses the update,
naming both plugins, before the question and again before anything is replaced. The plugin's own
installed offers are not counted. Nothing is replaced before the yes. On a yes the answers recorded for
the plugin's hooks in `hooks-approved` and `plugin` lines in `mcp-approved` are all withdrawn before the
directory is replaced, as `remove` does, so a withdrawal that cannot be written stops the update with the
old plugin in place. All of them are withdrawn and not only those of changed parts, because the digest of
each approval includes the plugin's tree ([PLUGIN-7](#PLUGIN-7)), so every hook and every server asks
again. The directory is replaced through `.<name>.updating` and `.<name>.previous` and the record
through a temporary file and a rename, and a failure after the swap puts the old directory back. The
record keeps whether the plugin is enabled and takes the new version, source, commit and digest. A
plugin whose directory no longer matches its record is still updated, since the record is the baseline.
No plugin updates on its own and no session start checks for one or makes a request for one.

`/plugin remove <name>` deletes the directory and its record, and withdraws what was recorded for its
hooks and servers: the lines of `hooks-approved` that name the plugin and the `plugin` lines of
`mcp-approved`, before anything is deleted, so a remove that cannot rewrite either file stops with the
plugin still installed. Installing the same files again is asked about again. `/plugin list` prints each
installed plugin's name, version, source, resolved commit, recorded digest and whether it is enabled,
reads nothing but that record and starts no program.

**Why.** An update that applied itself would be a fetch nobody asked for followed by changed programs
nobody saw.

`verified-by: bravebot_agent::plugins::list_shows_what_was_installed_from_the_records_alone`
`verified-by: bravebot_agent::plugins::remove_deletes_the_directory_and_the_record_and_nothing_else`
`verified-by: bravebot_tui::plugin_command::list_prints_name_version_source_digest_and_enabled`
`verified-by: bravebot_tui::plugin_command::list_prints_the_resolved_commit_of_a_git_source_and_none_for_a_path`
`verified-by: bravebot_tui::plugin_command::remove_enable_and_disable_act_without_a_question_and_miss_in_words`
`verified-by: bravebot_config::hooks_approved::withdrawing_a_plugin_removes_its_lines_and_leaves_the_others_as_they_are`
`verified-by: bravebot_agent::plugin_hooks::a_removed_plugin_is_asked_about_again_when_it_is_installed_again`
`verified-by: bravebot_agent::servers::plugin_tests::removing_a_plugin_withdraws_its_servers_answers_and_leaves_the_rest`
`verified-by: bravebot_tui::plugin_command::update_is_dispatched_and_says_what_it_did_in_words`
`verified-by: bravebot_agent::plugins::an_update_of_an_unchanged_local_source_changes_nothing_and_asks_nothing`
`verified-by: bravebot_agent::plugins::an_update_of_a_changed_local_source_asks_once_and_a_no_changes_nothing`
`verified-by: bravebot_agent::plugins::an_update_with_nobody_to_ask_or_a_bad_name_or_another_plugin_changes_nothing`
`verified-by: bravebot_agent::plugins::a_source_recorded_as_a_full_commit_has_nothing_to_update_to_and_asks_nothing`
`verified-by: bravebot_agent::plugins::an_update_puts_the_host_question_before_any_connection_and_a_no_makes_none`
`verified-by: bravebot_agent::plugins::an_update_fetches_the_recorded_ref_and_compares_the_commit_and_the_digest`
`verified-by: bravebot_agent::plugins::a_yes_replaces_the_plugin_keeps_it_disabled_and_withdraws_only_its_own_answers`
`verified-by: bravebot_agent::plugins::an_update_offering_a_name_another_plugin_offers_is_refused_naming_both`
`verified-by: bravebot_agent::plugins::an_update_that_cannot_be_completed_leaves_the_old_plugin_working`
`verified-by: bravebot_agent::plugins::an_update_after_a_crash_between_its_renames_puts_the_old_copy_back_first`
`verified-by: bravebot_agent::plugins::a_staged_tree_that_cannot_be_renamed_is_copied_into_place`
`verified-by: bravebot_agent::plugins::a_git_record_whose_url_no_longer_reads_as_a_git_source_is_unreadable`
`verified-by: bravebot_agent::plugins::the_files_that_differ_list_removed_files_and_directories_as_well_as_added_ones`
`verified-by: bravebot_agent::plugins::names_that_differ_only_in_invalid_bytes_stay_two_paths_in_the_file_map`
`verified-by: bravebot_tui::plugin_command::a_changed_source_puts_the_difference_and_only_a_typed_yes_updates`
`verified-by: bravebot_tui::plugin_command::every_permission_mode_still_puts_the_update_prompt_to_the_person`
`verified-by: bravebot_tui::plugin_command::an_update_into_a_name_another_plugin_offers_is_said_naming_both_and_asks_nothing`
`verified-by: bravebot_tui::confirm::plugin::update::the_prompt_shows_each_part_added_removed_and_changed_with_both_commits`
`verified-by: bravebot_tui::confirm::plugin::update::the_difference_leaves_out_what_is_equal_and_pairs_a_change`
`verified-by: bravebot_tui::confirm::plugin::update::a_side_that_could_not_be_read_is_said_and_not_shown_as_empty`
`verified-by: bravebot_tui::confirm::plugin::update::text_of_the_difference_is_drawn_behind_the_margin_and_written_out`
`verified-by: bravebot_tui::confirm::plugin::update::a_long_description_is_cut_and_a_yes_waits_for_it_and_argv_rows_are_never_cut`
`verified-by: bravebot_tui::confirm::plugin::update::only_a_y_updates_and_not_before_every_row_has_been_drawn`
`verified-by: bravebot_cli::running::starting_a_session_makes_no_request_for_an_installed_git_source_plugin`
`verified-by: bravebot_cli::running::a_one_shot_task_opening_with_slash_plugin_update_changes_nothing`

<a id="PLUGIN-10"></a>
### PLUGIN-10: a plugin's text reaches a turn only as the part it is

A skill's name and description reach the prompt as [skills.md](skills.md) says, and its body waits for
`load_skill`. A definition is a delegate's definition. The manifest's `description`, a README, and any
other file in the plugin are not read into a turn. Nothing the plugin says about itself is a source of
instructions.

**Why.** A plugin directory holds files that are not parts, and a reader that put them in a context
would be a second way for the plugin to speak that no prompt showed.

`verified-by: bravebot_agent::plugin_sources::nothing_but_the_parts_reaches_a_turn`
`verified-by: bravebot_agent::plugin_sources::an_enabled_plugin_offers_its_skill_and_definition_and_a_disabled_one_offers_nothing`

<a id="PLUGIN-11"></a>
### PLUGIN-11: a plugin can be turned off and kept

`/plugin disable <name>` sets the plugin's recorded state to disabled. A disabled plugin offers no
skill, no definition, no hook and no server, and its directory, its record and every answer recorded
for its hooks and servers stay as they were. `/plugin enable <name>` sets the state back and asks
nothing new: a part whose digest equals the one answered for is as approved as before, and any other
part asks as [PLUGIN-7](#PLUGIN-7) says. Both commands are typed ([PLUGIN-1](#PLUGIN-1)). A newly
installed plugin is enabled.

**Why.** A person who wants a plugin out of the way for a while should not have to remove it and answer
every question again.

`verified-by: bravebot_agent::plugins::disable_then_enable_rewrites_only_the_recorded_state`
`verified-by: bravebot_agent::plugin_hooks::a_plugin_disabled_after_a_yes_fires_nothing_and_keeps_its_answer`
`verified-by: bravebot_agent::plugin_hooks::enabling_an_unchanged_plugin_asks_nothing_and_a_changed_one_does_not_fire`
`verified-by: bravebot_agent::plugin_hooks::a_disabled_plugin_carries_no_hook`
`verified-by: bravebot_agent::servers::plugin_tests::a_disabled_plugin_starts_no_server_and_its_answers_stay`
`verified-by: bravebot_agent::plugins::a_disabled_plugin_is_listed_and_offers_nothing_until_enabled`
`verified-by: bravebot_agent::plugin_sources::an_enabled_plugin_offers_its_skill_and_definition_and_a_disabled_one_offers_nothing`
`verified-by: bravebot_tui::plugin_command::remove_enable_and_disable_act_without_a_question_and_miss_in_words`

<a id="PLUGIN-12"></a>
### PLUGIN-12: a plugin's skills and definitions rank last, and two plugins never offer the same name

A skill or definition from a plugin is the least specific source of its kind. A file of the same name
from `~/.bravebot`, a directory the person named ([INSTR-17](instructions.md#INSTR-17)) or a project
shadows it, by the shadowing of [INSTR-4](instructions.md#INSTR-4). A name offered by an installed
plugin that another plugin's part also carries is refused: the install names both plugins and stops,
and a person removes or disables one first. A plugin's skills are not given a prefix naming the plugin.

**Why.** What a person wrote by hand should win over what a stranger's plugin carries under the same
name, and a rule that picks between two plugins would make which one answers depend on the order they
were installed in.

`verified-by: bravebot_agent::plugin_sources::a_plugin_skill_is_shadowed_by_every_other_source_of_its_name`
`verified-by: bravebot_agent::plugin_sources::a_plugin_definition_is_shadowed_by_every_other_source_of_its_name`
`verified-by: bravebot_agent::plugins::a_second_plugin_offering_an_offered_name_is_refused_naming_both_and_changes_nothing`
`verified-by: bravebot_agent::plugins::two_plugins_offering_one_definition_name_conflict`
`verified-by: bravebot_tui::plugin_command::a_plugin_skill_named_like_a_home_skill_is_marked_and_the_home_one_is_offered`
`verified-by: bravebot_agent::plugin_sources::a_plugin_skill_is_held_and_listed_as_a_home_skill_is`
`verified-by: bravebot_agent::plugin_sources::a_plugin_definition_is_listed_as_a_home_definition_is`
`verified-by: bravebot_tui::render::a_plugin_skill_row_says_it_came_from_a_plugin`

<a id="PLUGIN-13"></a>
### PLUGIN-13: a part may name its own directory with one variable

In the argv of a hook and the command and arguments of a server, the text `${PLUGIN_ROOT}` is replaced
with the absolute path of `~/.bravebot/plugins/<name>`. It is replaced before the digest of
[PLUGIN-7](#PLUGIN-7) is computed and before the install listing is drawn, so the listing and the
approval show the path that will run. No other variable is replaced, the text is not looked up in the
environment, and a `${PLUGIN_ROOT}` anywhere else in a part is left as written. A plugin has no
directory for data of its own.

**Why.** A hook that runs a script the plugin carries cannot be written without a path to the plugin.
Expanding it in the listing keeps what the person approved identical to what runs.

`verified-by: bravebot_config::plugin::every_occurrence_in_a_word_is_replaced_once_and_nothing_else_is`
`verified-by: bravebot_config::plugin::a_hook_argv_names_the_directory_the_plugin_will_have_not_the_one_it_was_read_from`
`verified-by: bravebot_config::plugin::the_argv_a_digest_is_taken_over_is_the_expanded_one_and_follows_the_install_directory`
`verified-by: bravebot_config::plugin::the_variable_is_left_as_written_anywhere_but_an_argv_word`
`verified-by: bravebot_tui::confirm::plugin::hooks_and_servers_are_listed_in_full_with_the_installed_path_and_their_reach`

<a id="PLUGIN-14"></a>
### PLUGIN-14: a source may name a ref, and the commit it resolved to is recorded

A git source may end in `#<ref>`, a branch, a tag or a full commit. The install records the ref asked for
and the full commit it resolved to, shows both ([PLUGIN-4](#PLUGIN-4)), and records the tree digest as
[PLUGIN-5](#PLUGIN-5) says. `/plugin update` fetches the recorded ref again and compares the commit and
the digest. A source naming a full commit has nothing to update to. A local path has no commit and is
recorded with its digest.

**Why.** Without an index to pin a source, the commit the person was shown is the pin, and an update is
a different commit the person is shown again.

`verified-by: bravebot_agent::plugin_source::a_git_url_is_split_from_its_ref_at_the_first_hash`
`verified-by: bravebot_agent::plugin_source::a_ref_that_git_could_read_as_an_option_or_that_holds_a_control_character_is_refused`
`verified-by: bravebot_agent::plugin_source::a_url_with_a_control_character_is_refused_and_a_commit_is_forty_lowercase_hex_digits`
`verified-by: bravebot_agent::plugin_git::a_branch_a_tag_a_full_commit_and_no_ref_each_record_the_commit_they_resolve_to`
`verified-by: bravebot_agent::plugins::a_yes_to_the_host_and_to_the_listing_installs_the_commit_the_branch_pointed_to`
`verified-by: bravebot_agent::plugins::a_git_record_holds_the_url_ref_and_commit_and_the_same_url_and_ref_is_an_update`
`verified-by: bravebot_tui::confirm::plugin::a_git_source_shows_the_requested_ref_and_the_full_commit`
`verified-by: bravebot_tui::plugin_command::list_prints_the_resolved_commit_of_a_git_source_and_none_for_a_path`
`verified-by: bravebot_agent::plugins::an_update_fetches_the_recorded_ref_and_compares_the_commit_and_the_digest`
`verified-by: bravebot_agent::plugins::a_source_recorded_as_a_full_commit_has_nothing_to_update_to_and_asks_nothing`
`verified-by: bravebot_agent::plugins::an_update_of_a_changed_local_source_asks_once_and_a_no_changes_nothing`

## What this changes elsewhere

These are the places a built spec says something this spec would need to extend, each decided when the
work lands, in the same commit as the code.

- **Hooks.** Done: the hooks spec says one file in `~/.bravebot` declares the person's hooks, names the
  plugin exception in [HOOK-1](hooks.md#HOOK-1), and has the clause for the question in
  [HOOK-11](hooks.md#HOOK-11). A plugin's hooks file is read from the plugin's directory under that
  state directory, and its entries fire only after the question in [PLUGIN-7](#PLUGIN-7), recorded in
  `~/.bravebot/hooks-approved`.
- **Skills and definitions.** Each source list gains `~/.bravebot/plugins/<name>/...` as its least
  specific entry ([PLUGIN-12](#PLUGIN-12)), with the trust of the sibling `~/.bravebot` source. Done:
  [INSTR-1](instructions.md#INSTR-1), [INSTR-4](instructions.md#INSTR-4),
  [INSTR-17](instructions.md#INSTR-17), [SKILL-3](skills.md#SKILL-3) and
  [DELEGATE-20](delegation.md#DELEGATE-20) say so.
- **MCP servers.** Done: [SERVERS-1](mcp-servers.md#SERVERS-1) names the declaration file as
  `~/.bravebot/mcp.json` and no other with the exception of a plugin's servers file, which is read
  alongside it with the same first-use question and its own digest ([SERVERS-27](mcp-servers.md#SERVERS-27)).
- **The trust map.** It does not govern the plugin directory, for the reason it does not govern the rest
  of `~/.bravebot`. Done: [TRUST-11](trust-map.md#TRUST-11) says so.
- **Programs bravebot starts.** The `git` that clones a source is added to the programs
  [SANDBOX-30](sandboxing.md#SANDBOX-30) lists. Done: that clause lists it, with its variable table.
- **Import.** A conversion of another tool's plugin layout would belong to [import.md](import.md). Nothing
  in this spec reads one.

## Known costs

- A person installing a plugin sees a long listing. The listing is the protection, and a short one would
  be the plugin's own summary.
- A plugin cannot ship a permission rule, so the readme of a plugin that wants a command allowed has to
  tell a person to write the rule.
- No project scope, no "required for a team" setting and no managed list of allowed sources exist. A
  team shares a plugin by sharing its source and every person runs the install.
- Nothing lists plugins to choose from. A person finds a source elsewhere, and a pin says the bytes are
  the ones shown, not that they are good.
- A plugin's own programs have no directory for data and no per-plugin settings.
- A `${PLUGIN_ROOT}` is replaced by the literal text of the install directory for a hook's argv and a
  server's command and arguments, and every install and every start hashes the plugin's whole directory
  again. A large plugin pays that read before each hook moment and each server start. A person who wants
  a variable in an environment value or a url writes the path out, since nothing else is replaced.
- A notice that a plugin no longer matches its record is a plain sentence in the transcript. No front
  end draws a notice through the margin, and the plugin's name is held to letters, digits and hyphens
  at install, so it carries no control character. The text a plugin wrote is drawn through the margin in
  the install listing and in the hook question.
- A plugin's hooks and servers file is read as `hooks.json` and `mcp.json` are, so an entry the hook
  reader cannot use is left out and counted in the install prompt without its text, and a servers file
  the server reader refuses lists no server and says so.
- A plugin update fetches the source again only when a person types `/plugin update <name>`. Nothing
  checks for a newer version, so a plugin whose source has moved stays as installed until then, and a
  person who wants to know whether one has moved runs the command, which asks nothing when the files
  are the same. A source recorded at a full commit cannot be updated: it is removed and added again at
  another commit. An update withdraws every recorded answer for the plugin's hooks and servers and not
  only those of changed parts, so an unchanged hook asks again.
- A local path is updated by reading the recorded path again, with no host question, and an update does
  not hash the installed directory first, so a directory edited after the install is replaced by an
  update. A new commit whose files are the same changes nothing and is not recorded.
- An update replaces the directory with two renames, so a crash between them leaves the old copy under
  `.<name>.previous`, which the next update puts back before it begins. A failure after the withdrawal
  leaves the old plugin working and asking again for its hooks and servers.
- A plugin hook that fires only on `session-ended` or `prompt-shown` fires only after an earlier yes,
  a `file-changed` question waits while the person has a draft, and the desktop interface, an editor
  over ACP and the local host cannot approve a plugin hook, so none runs there
  ([HOOK-11](hooks.md#HOOK-11)).
- A plugin's server is not moved when its host redirects ([SERVERS-27](mcp-servers.md#SERVERS-27)).
- `bravebot doctor` says whether OpenShell can start the servers declared for it from `mcp.json` alone,
  so a server a plugin carries that declares the OpenShell backend is not reported there
  ([SERVERS-24](mcp-servers.md#SERVERS-24), [SERVERS-27](mcp-servers.md#SERVERS-27)).
- A plugin's server keeps its home directory under `~/.bravebot/mcp-home`, named by the digest of the
  declaration as expanded and not by the plugin's name, so two plugin servers whose expanded declarations
  are identical share one directory ([SERVERS-10](mcp-servers.md#SERVERS-10),
  [SERVERS-27](mcp-servers.md#SERVERS-27)).
- `bravebot mcp approve` reads `mcp.json` only and approves no server a plugin carries. A plugin's server
  is approved by a yes at its first-use question in a session at a terminal, and `bravebot mcp get` says
  so ([SERVERS-27](mcp-servers.md#SERVERS-27)).
- A confined stage may write the system temporary directory ([SANDBOX-30](sandboxing.md#SANDBOX-30)),
  so a stage running at the same time as a clone can write in the clone's directory. The clone's own
  settings that matter for what runs are on its command line, which a file there does not outrank, and
  the copy that is listed and installed is made after the clone ends. Settings only a file could carry
  (for example `core.sshCommand` in the clone's git directory) are not protected during the fetch. A
  person who needs that closed runs the install with no other stage running. Moving the clone outside
  every directory a stage may write would need a directory under the state directory, which
  [PLUGIN-3](#PLUGIN-3) excludes.
- An `https` URL is a git source only when its path ends in `.git`, because deciding otherwise would
  take a request before the host question. A host that serves git at another path is reached with an
  `ssh` URL or with `.git` added.
- The clone does not use `GIT_SSH_COMMAND`, `GIT_ASKPASS` and the other variables that name a
  program ([SANDBOX-30](sandboxing.md#SANDBOX-30)). A person whose `ssh` needs one sets it in their
  own git or ssh configuration, which is read.
- A clone's resolved commit is taken from the output of the `git` program and accepted only when it
  is 40 lowercase hex digits. Any other output is a refusal (`CloneFailed`). This shape check decides
  nothing else, and the driver takes nothing else from git's output. The commit is carried to the
  prompt and the record.
- The `https` and `ssh` clone paths are tested against `file` and `http` stand-ins, not against a real
  TLS or ssh server.
- A plugin written for another tool does not install here, because only `plugin.toml` is read.
- The install decides from a plugin's bytes in three places, and each only refuses or marks: the manifest
  reader refuses a decision key, a part path that leaves the root and a bad name; the shadow marking
  compares a skill or definition name with the session's own list and marks the row; the two-plugin
  comparison refuses an install whose name another plugin offers. An update decides from the new bytes in
  four more places, and each only refuses, marks or lists: it compares the manifest name of the new tree
  with the installed plugin's name and refuses a source that now holds another plugin, it compares the
  new tree's digest with the recorded one to decide whether to ask at all, it compares the new and the
  installed copies' parts and files to draw the difference, and it reads whether the installed manifest
  could be read (`Before.readable`) so that the listing says an unreadable copy is unreadable and does
  not show it as empty. The comparison produces rows of the program's own wording and the
  plugin's text, drawn behind the margin, and the answer is the person's. Each result is a sentence of the
  program's own wording drawn to the person, and the plugin's text is not in it unmarked. None runs in a
  turn or reaches the planner's context, so none is one of the places [labels.md](labels.md) lists
  under Known costs, which decide from untrusted bytes inside a turn.
- The manifest and the part files a plugin names decide only a refusal at stage: a name conflict with
  another plugin, a part that is missing or cannot be read, and a link that leaves the plugin. A hooks
  or servers file that cannot be read is said in the listing and offers nothing. None of them starts a
  program, approves an answer or chooses what a turn does.
