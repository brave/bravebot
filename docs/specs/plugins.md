---
id: PLUGIN
title: Plugins
status: proposed
governs:
  - crates/config/src/hooks.rs
  - crates/config/src/mcp.rs
  - crates/config/src/plugin.rs
  - crates/agent/src/plugins.rs
  - crates/tui/src/plugin_command.rs
  - crates/tui/src/confirm/plugin.rs
documented-by:
  - docs/website/docs/customize/plugins.md
---

## Scope

One person-run command that installs a bundle of skills, delegate definitions, hooks and MCP server
declarations from one source, what the person is shown before it does, where each part lands, what a
plugin can never carry, and how an installed plugin is updated, turned off and removed.

What is built is the install of a plugin from a local path with `/plugin add`, and `/plugin list`,
`/plugin remove`, `/plugin enable` and `/plugin disable`, for the skills and definitions a plugin
carries. Not built are cloning a git source and a ref ([PLUGIN-3](#PLUGIN-3),
[PLUGIN-14](#PLUGIN-14)), `/plugin update` ([PLUGIN-9](#PLUGIN-9)), the hooks and servers parts and
their first-use questions ([PLUGIN-7](#PLUGIN-7)), and `${PLUGIN_ROOT}` ([PLUGIN-13](#PLUGIN-13)). A
clause reads `verified-by: none` where none of it is built, and a clause only partly built has a
**Not built** paragraph naming what is missing. The specs this one changes are named in
[What this changes elsewhere](#what-this-changes-elsewhere). The skills, definitions, instructions
and trust-map specs were edited when the install landed, and the hooks, MCP servers, programs and
import specs are not edited until the work they describe lands.

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

**Not built:** `/plugin update`. The typed `/plugin update` is refused with a notice that says it is
not built.

`verified-by: bravebot_agent::tools::no_tool_offered_installs_or_changes_a_plugin`
`verified-by: bravebot_tui::app::the_plugin_command_carries_its_words_unparsed`
`verified-by: bravebot_tui::app::a_pasted_plugin_line_is_text_in_the_box_and_not_the_command`
`verified-by: bravebot_cli::running::a_one_shot_task_opening_with_slash_plugin_add_installs_nothing`

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

**Why.** A reader that finds parts by folder name installs whatever the author left lying in the
checkout. A part the person was shown has to be one the manifest named. A second manifest format would
need a translation from another tool's hook moments and server fields into this one's, and the person
would be shown the translation rather than the file.

**Not built:** a `hooks` or `servers` file is read as data and listed, and nothing installs it.

`verified-by: bravebot_config::plugin::a_manifest_gives_its_name_version_description_and_parts`
`verified-by: bravebot_config::plugin::a_part_the_manifest_does_not_name_is_none`
`verified-by: bravebot_config::plugin::hooks_and_servers_are_read_and_not_installed`
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

A git URL is cloned shallow into a staging directory outside `~/.bravebot` and outside every workspace.
The host is shown and asked about through the same prompt a fetch uses, and no request leaves before
the yes. Only `https` and `ssh`-style git URLs and local paths are sources. A redirect to another host
is not followed. A source that is an archive, an npm package or a plain URL is refused.

The clone is made by the `git` program, started the way [SANDBOX-30](sandboxing.md#SANDBOX-30)
requires, with the variables that select code removed from its environment, no submodule fetched and no
hook from the cloned repository run. A local path is read in place, copied into staging and never
executed there. As built, the staging directory for a local path is a `.plugin-staging-*` directory
inside `~/.bravebot`, removed when the install ends, is refused or is declined.

The staged bytes are untrusted content. Nothing in them is read by the driver or the planner before the
person has seen what [PLUGIN-4](#PLUGIN-4) lists, and the listing is built by a reader that parses the
manifest and the parts as data. No part of the staged tree is executed, loaded or followed to decide what
is listed.

**Why.** The download is the first moment untrusted bytes exist, and the person's yes to the host is
the same decision a fetch of a page asks. A staged tree that something already ran would have decided
for the person.

**Not built:** the clone of a git URL, the host question, the redirect rule and the environment of the
`git` program. A local path is the only source, and it is copied into staging without being executed.

`verified-by: none`

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

**Why.** A person cannot vouch for a thing they were not shown, and a prompt listing the plugin's own
summary of itself would let the plugin choose what the person reads. The prompt is built from the
parts, not from a field the manifest wrote.

**Not built:** the hook and server rows, since neither is installed, the host, command and path rows
that only a hook or a server reaches, and the resolved commit, which a local path has none of.

`verified-by: bravebot_tui::confirm::plugin::the_prompt_lists_every_part_and_says_what_a_yes_does_not_do`
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
both sources. An install of the same source is an update ([PLUGIN-9](#PLUGIN-9)).

There is no project scope and no setting that makes a plugin required for a team.

**Why.** The state directory is the one place a clone cannot reach, which is the property every
single-file spec of the declarations depends on. A project scope would put plugin parts where a checkout
authors them.

**Not built:** the resolved ref, which only a git source has ([PLUGIN-14](#PLUGIN-14)), and the update
that an install of the same source would be. Adding the same source again is refused and says update
is not built.

`verified-by: bravebot_agent::plugins::an_install_puts_the_tree_and_a_record_under_the_plugins_directory`
`verified-by: bravebot_agent::plugins::a_name_installed_from_another_source_is_refused_naming_both_and_changes_nothing`
`verified-by: bravebot_agent::plugins::adding_the_same_source_again_is_refused_because_update_is_not_built`
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

**Not built:** a hook is not installed, so what it may say is not yet in question.

`verified-by: bravebot_config::plugin::a_manifest_with_a_decision_key_is_refused_by_read_and_nothing_is_made`
`verified-by: bravebot_config::plugin::a_key_that_only_resembles_a_decision_key_is_not_one`
`verified-by: bravebot_config::plugin::the_decision_list_and_the_keys_read_do_not_overlap`
`verified-by: bravebot_config::plugin::the_manifest_has_no_field_that_can_carry_a_decision`
`verified-by: bravebot_config::plugin::a_decision_key_refuses_the_manifest_whole_however_it_is_written`
`verified-by: bravebot_core::delegate::a_definition_can_only_narrow_what_its_kind_holds`

<a id="PLUGIN-7"></a>
### PLUGIN-7: installing a part does not approve it

A skill from a plugin is trusted for what a skill in `~/.bravebot/skills` is trusted for, and no more.
A definition from a plugin is one `~/.bravebot/agents` would hold. A server from a plugin is reachable
only after the first-use question that names its digest, as a server added by `bravebot mcp add` is,
and the install records no answer to it.

A hook from a plugin fires only after the person has answered a question naming its moment and its
argv as [PLUGIN-13](#PLUGIN-13) expands it. The question is asked at the first moment the hook would
fire, in a session a person is watching, and at most once in a session. A yes is recorded in
`~/.bravebot/hooks-approved` as a digest of the plugin's name, the hook's moment, its expanded argv and
the plugin's tree digest. A no records nothing, the hook does not fire, and the question is asked again
in a later session. Where nobody can be asked, the hook does not fire.

Before a hook fires or a server starts, the plugin's directory is hashed again. A tree whose digest is
not the one recorded at install stops the hook or the server, and the person is told which plugin no
longer matches its record. Changing a hook's moment or argv, or any file in the plugin, is a question
again after an update the person accepted.

**Why.** The install is one yes to a list. Running a program the first time is a decision about that
program, and it is asked about where it runs, in a session the person is watching. A hook is usually an
argv that starts a script in the plugin, so an approval bound to the argv alone would still hold after
the script changed.

**Not built:** the hook and server parts of this clause. Nothing installs a hook or a server, so there is
no first-use question, no `hooks-approved` record and no digest check before one runs. The skill and
definition sentences hold, as [PLUGIN-12](#PLUGIN-12) says.

`verified-by: none`

<a id="PLUGIN-8"></a>
### PLUGIN-8: withdrawn, there is no marketplace

Replaced by [PLUGIN-14](#PLUGIN-14) and [PLUGIN-5](#PLUGIN-5). No index of plugins exists and none is
registered. A source is a git URL or a local path the person names, and the pin that an index entry
would carry is the resolved commit and the tree digest those two clauses record.

<a id="PLUGIN-9"></a>
### PLUGIN-9: update, remove and list work from the recorded source

`/plugin update <name>` fetches the recorded source again at the recorded ref
([PLUGIN-3](#PLUGIN-3), [PLUGIN-14](#PLUGIN-14)) and, when the tree's digest differs, puts the
difference to the person the way [PLUGIN-4](#PLUGIN-4) puts a first install: each part added, removed or
changed, in full. A part whose name another installed plugin already offers ([PLUGIN-12](#PLUGIN-12))
refuses the update, naming both plugins. Nothing is replaced before the yes, and an answer given to a
hook or a server before is withdrawn for any part whose digest changed. No plugin updates on its own and
no session start checks for one.

`/plugin remove <name>` deletes the directory and its record, and withdraws what was recorded for its
hooks and servers. `/plugin list` prints each installed plugin's name, version, source, resolved commit,
recorded digest and whether it is enabled, reads nothing but that record and starts no program.

**Why.** An update that applied itself would be a fetch nobody asked for followed by changed programs
nobody saw.

**Not built:** `/plugin update`, so the fetch again, the difference put to the person and the withdrawal of
an answer are not built. `/plugin remove` and `/plugin list` are built. Nothing is recorded for a hook or
a server, so remove has nothing of those to withdraw.

`verified-by: bravebot_agent::plugins::list_shows_what_was_installed_from_the_records_alone`
`verified-by: bravebot_agent::plugins::remove_deletes_the_directory_and_the_record_and_nothing_else`
`verified-by: bravebot_tui::plugin_command::list_prints_name_version_source_digest_and_enabled`
`verified-by: bravebot_tui::plugin_command::remove_enable_and_disable_act_without_a_question_and_miss_in_words`
`verified-by: none`

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

**Not built:** a hook or a server is not installed, so there is no answer recorded for one to keep.

`verified-by: bravebot_agent::plugins::disable_then_enable_rewrites_only_the_recorded_state`
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

**Not built:** the clause. No part with an argv is installed, so nothing replaces `${PLUGIN_ROOT}`.

`verified-by: none`

<a id="PLUGIN-14"></a>
### PLUGIN-14: a source may name a ref, and the commit it resolved to is recorded

A git source may end in `#<ref>`, a branch, a tag or a full commit. The install records the ref asked for
and the full commit it resolved to, shows both ([PLUGIN-4](#PLUGIN-4)), and records the tree digest as
[PLUGIN-5](#PLUGIN-5) says. `/plugin update` fetches the recorded ref again and compares the commit and
the digest. A source naming a full commit has nothing to update to. A local path has no commit and is
recorded with its digest.

**Why.** Without an index to pin a source, the commit the person was shown is the pin, and an update is
a different commit the person is shown again.

**Not built:** a git source, a ref, the resolved commit and the update. A local path is recorded with
its digest ([PLUGIN-5](#PLUGIN-5)).

`verified-by: none`

## What this changes elsewhere

These are the places a built spec says something this spec would need to extend, each decided when the
work lands, in the same commit as the code.

- **Hooks.** The hooks spec says one file in `~/.bravebot` declares hooks and no other file does. A
  plugin's hooks file is read from the plugin's directory under that state directory, and its entries
  fire only after the question in [PLUGIN-7](#PLUGIN-7), recorded in `~/.bravebot/hooks-approved`. The
  hooks spec needs a clause for that question, since today it has none.
- **Skills and definitions.** Each source list gains `~/.bravebot/plugins/<name>/...` as its least
  specific entry ([PLUGIN-12](#PLUGIN-12)), with the trust of the sibling `~/.bravebot` source. Done:
  [INSTR-1](instructions.md#INSTR-1), [INSTR-4](instructions.md#INSTR-4),
  [INSTR-17](instructions.md#INSTR-17), [SKILL-3](skills.md#SKILL-3) and
  [DELEGATE-20](delegation.md#DELEGATE-20) say so.
- **MCP servers.** The declaration file is `~/.bravebot/mcp.json` and no other, and a plugin's server
  list is read alongside it with the same digest and first-use question.
- **The trust map.** It does not govern the plugin directory, for the reason it does not govern the rest
  of `~/.bravebot`. Done: [TRUST-11](trust-map.md#TRUST-11) says so.
- **Programs bravebot starts.** The `git` that clones a source is added to the programs
  [SANDBOX-30](sandboxing.md#SANDBOX-30) lists.
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
- A plugin written for another tool does not install here, because only `plugin.toml` is read.
- The install decides from a plugin's bytes in three places, and each only refuses or marks: the manifest
  reader refuses a decision key, a part path that leaves the root and a bad name; the shadow marking
  compares a skill or definition name with the session's own list and marks the row; the two-plugin
  comparison refuses an install whose name another plugin offers. Each result is a sentence of the
  program's own wording drawn to the person, and the plugin's text is not in it unmarked. None runs in a
  turn or reaches the planner's context, so none is one of the places [labels.md](labels.md) lists
  under Known costs, which decide from untrusted bytes inside a turn.
