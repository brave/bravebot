---
id: SKILL
title: Skills and standing instructions
status: normative
governs:
  - crates/agent/src/skills.rs
guards:
  - symbol: Policy::label_user_configuration
  - symbol: Policy::read_trusted_content
documented-by: docs/website/docs/customize/skills.md
---

## Scope

The two kinds of file that steer a turn before the user types anything: `AGENTS.md`, which says
how work is done somewhere, and a skill, which says how one kind of task is done. What a skill
file looks like, what each source is trusted for, and how one is loaded.

Which files are looked for, where, and in what order is a separate question, and it is
[instructions.md](instructions.md) that answers it.

## Clauses

<a id="SKILL-1"></a>
### SKILL-1: a skill is one `SKILL.md` with `name` and `description` in frontmatter

Both keys are required, and a file missing either is skipped with a note saying so. A key nothing
here reads stops nothing, so a skill written for another agent works here, and
[SKILL-16](#SKILL-16) says where such a key is reported rather than left a silent no-op.
[SKILL-15](#SKILL-15) is the two keys read for how the skill runs, and [SKILL-20](#SKILL-20) is the
key that keeps a skill from the planner. `argument-hint` is read too: an interface draws it after the
skill's name to say what the skill takes ([commands.md](commands.md)), and it is never given to the
planner. A file with no frontmatter is not a skill.

```markdown
---
name: commit-style
description: How commit messages are written here. Use before writing one.
---

Write the subject in the imperative. Explain why in the body, never what.
```

`verified-by: bravebot_agent::skills::frontmatter_without_a_name_or_description_is_skipped`
`verified-by: bravebot_agent::skills::a_file_with_no_frontmatter_is_not_a_skill`
`verified-by: bravebot_agent::skills::an_unterminated_frontmatter_block_is_skipped_rather_than_swallowing_the_body`
`verified-by: bravebot_agent::skills::a_key_nothing_here_reads_does_not_stop_a_skill_and_is_carried_out`
`verified-by: bravebot_agent::skills::an_argument_hint_is_read_when_the_file_has_one`
`verified-by: bravebot_agent::skills::an_argument_hint_reaches_the_interface_and_not_the_planner`
`verified-by: bravebot_agent::skills::the_body_is_everything_after_the_closing_marker`
`verified-by: bravebot_agent::skills::a_marker_inside_the_body_is_left_alone`

<a id="SKILL-2"></a>
### SKILL-2: only the name and description reach the planner; the body waits for `load_skill`

Whether a skill is listed in the prompt or found with `search_skills` ([SKILL-18](#SKILL-18)),
what reaches the planner before `load_skill` is the name and the description, never the body. The
description is what the planner decides from, so it should say *when* to use the skill rather
than what it contains. A skill kept from the planner ([SKILL-20](#SKILL-20)) is the one exception: of
that skill not even the name and description reach the planner.

**Why.** A directory of long skills would otherwise crowd out the task.

`verified-by: bravebot_agent::skills::what_the_prompt_advertises_holds_no_bodies`
`verified-by: bravebot_agent::turn::a_skill_body_stays_out_of_the_context_until_search_finds_it_and_load_skill_is_asked`
`verified-by: bravebot_agent::skills::skills_are_offered_in_the_same_order_every_time`

<a id="SKILL-3"></a>
### SKILL-3: `~/.bravebot` is trusted by provenance, never by the trust map

It is labelled from provenance, because the trust map is keyed by workspace-relative paths and has
nothing to say about a path outside the workspace. Never label a workspace path this way: that
would be laundering.

Nothing is assumed from silence. An empty directory offers nothing, and putting a file there is
the grant, on the same footing as the configuration that picks the model and the endpoint.

The same provenance label applies to `~/.agents/skills` and `~/.claude/skills`, but only for a
directory the person's settings name ([INSTR-17](instructions.md#INSTR-17)); an unnamed one is not
read, so a file put there is no grant. The workspace exception below applies to each directory read
this way, and the notice names the directory it counts.

A skill file there whose path goes through the workspace is not read from provenance at all,
because a turn can write it. The workspace here is every directory the file tools may write to:
the working directory, a directory opened beside it ([TRUST-9](trust-map.md#TRUST-9)), and the
session's own ([TRUST-16](trust-map.md#TRUST-16)). Every file there goes through it where one of
those is, or holds, the home directory. So does a file reached through a link that sits inside the
workspace, wherever that link leads, since a turn can change the link. Such a file is the
project's, and is offered only where [SKILL-4](#SKILL-4) reads it through the trust map: in a
session started in the home directory, that is `.bravebot/skills`. Those passed over are counted
in a notice, as [SKILL-6](#SKILL-6) counts what it skips, except where `~/.bravebot/skills` is
itself one of the project skill directories read, whose own notices already account for each.
The two are compared as directories, so no entry's name is held or compared.

The same holds for the other files read from `~/.bravebot`. `~/.bravebot/AGENTS.md` whose path goes
through the workspace is not put in the system prompt, and the person is told it was not read.
A file in `~/.bravebot/agents` whose path goes through it is not a definition, and those passed
over are counted as skills are ([DELEGATE-20](delegation.md#DELEGATE-20)). A file in
`~/.bravebot/styles` whose path goes through it is not a style, is not offered by `/style` and is
not found by name.

`verified-by: bravebot_agent::skills::a_home_skill_is_not_labelled_by_a_rule_meant_for_the_workspace`
`verified-by: bravebot_agent::skills::a_session_started_in_the_home_directory_reads_its_skills_through_the_trust_map`
`verified-by: bravebot_agent::skills::a_skill_inside_a_working_directory_that_holds_the_home_directory_is_not_offered`
`verified-by: bravebot_agent::skills::a_home_directory_reached_through_a_link_into_the_workspace_is_inside_it`
`verified-by: bravebot_agent::skills::a_home_skill_named_inside_the_workspace_is_not_offered_through_a_link_out_of_it`
`verified-by: bravebot_agent::skills::a_home_skill_reached_through_a_link_inside_the_workspace_is_not_offered`
`verified-by: bravebot_agent::skills::a_home_skill_inside_a_directory_opened_beside_the_project_is_not_offered`
`verified-by: bravebot_agent::skills::a_home_directory_spelled_in_another_case_is_inside_the_workspace_it_is`
`verified-by: bravebot_agent::skills::a_home_skill_passed_over_is_counted_beside_a_project_skill_of_the_same_name`
`verified-by: bravebot_agent::preamble::a_home_agents_file_inside_the_working_directory_is_not_read_as_the_persons_own`
`verified-by: bravebot_agent::agents::a_definition_in_a_working_directory_that_holds_the_home_directory_is_not_loaded`
`verified-by: bravebot_agent::styles::a_style_file_the_workspace_can_write_is_not_read_as_the_persons_own`

<a id="SKILL-4"></a>
### SKILL-4: a project's own files are read through the trust map

A workspace `AGENTS.md` and a project's skill directories are labelled as workspace content, so
TRUST decides. Each skill directory is checked for trust **before it is enumerated at all**,
because a directory name is content too. Which directories those are, and in what order, is
[INSTR-13](instructions.md#INSTR-13).

The directories of another agent in the person's home are not read through the trust map. They are
read by provenance for a directory the person named ([INSTR-17](instructions.md#INSTR-17),
[SKILL-3](#SKILL-3)).

`verified-by: bravebot_agent::skills::a_skill_in_an_untrusted_project_is_not_named_to_the_planner`
`verified-by: bravebot_agent::skills::a_skill_the_trust_map_distrusts_stops_being_offered`
`verified-by: bravebot_agent::skills::a_foreign_skill_in_an_untrusted_project_is_counted_and_not_named`
`verified-by: bravebot_agent::skills::an_untrusted_entry_named_like_a_vouched_skill_is_still_counted`
`verified-by: bravebot_agent::skills::a_skill_from_no_vouched_root_leaves_an_untrusted_entry_counted`

<a id="SKILL-5"></a>
### SKILL-5: a source that fails `read_trusted_content` is dropped entirely, never quarantined

Both `~/.bravebot` and a workspace source pass the trusted-content gate on the way into the system
prompt, and a refusal drops the source.

**Why.** A reference to an instruction is no use to anyone: an instruction is either followed or
absent, and one from a directory nobody vouched for has to be absent. A skill's name and
description are content that would otherwise go into the prompt verbatim.

`verified-by: bravebot_agent::skills::every_source_reaches_the_prompt_through_the_trusted_content_gate`
`verified-by: by-construction (nothing untrusted reaches the gate, so no test can make it refuse: the home path labels its own text (T,pub), and a workspace file the trust map does not vouch for is dropped by the per-file check before the gate is called)`

<a id="SKILL-6"></a>
### SKILL-6: what was skipped is counted, never named

```
AGENTS.md was not loaded: this directory is not trusted
2 skills in .bravebot/skills were not loaded: this directory is not trusted
```

**Why.** A directory in an untrusted project can be named to read like an instruction, and that
name would be on the user's screen as though the agent had written it.

`verified-by: bravebot_agent::skills::a_skill_that_was_skipped_is_counted_rather_than_passed_over_in_silence`
`verified-by: bravebot_agent::skills::an_untrusted_skill_is_not_named_in_what_the_user_is_told`
`verified-by: bravebot_agent::skills::several_untrusted_skills_are_counted_and_none_of_them_is_named`

<a id="SKILL-7"></a>
### SKILL-7: withdrawn, the most specific source wins

Replaced by [instructions.md](instructions.md), which owns the order sources are read in along
with the rest of resolution.

<a id="SKILL-8"></a>
### SKILL-8: `load_skill`'s name is never a path

It selects from the set found before the turn started, so a name holding `../` or an absolute path
matches nothing and the call is refused: there is no lookup for it to reach. A name merely close to
a real one is refused too, rather than guessed at, since guessing would load instructions nobody
asked for.

`verified-by: bravebot_agent::turn::loading_a_skill_that_does_not_exist_is_refused_rather_than_guessed`

<a id="SKILL-9"></a>
### SKILL-9: withdrawn, sources are looked for afresh every turn

Replaced by [instructions.md](instructions.md), which owns when resolution runs.

<a id="SKILL-10"></a>
### SKILL-10: a value may be wrapped over the lines indented beneath it

A description says *when* to use a skill, so it runs to a sentence or two and files wrap it. The
lines indented under a key continue that key's value however the file spells the wrap: folded or
literal with `>` or `|`, quoted and carried over, or plain text simply continued. A folded value
is joined with spaces, because the line breaks were the file's and not the sentence's, and a
literal one keeps the newlines it asked for, including a blank line between two of its lines.
Quotes around a value are the file's syntax and are not part of it.

**Why.** A continuation line is not a declaration. Reading one as a key ends the value where the
line ended, which puts half a sentence in the prompt or, where the wrap began immediately after
the colon, an empty value and a skill dropped for being half-declared.

`verified-by: bravebot_agent::skills::a_value_wrapped_over_several_lines_is_one_value`
`verified-by: bravebot_agent::skills::a_continuation_line_holding_a_colon_does_not_start_a_new_key`
`verified-by: bravebot_agent::skills::a_folded_block_becomes_one_line_and_a_literal_block_keeps_its_own`
`verified-by: bravebot_agent::skills::a_blank_line_inside_a_literal_block_is_kept`
`verified-by: bravebot_agent::skills::the_quotes_around_a_scalar_are_not_part_of_it`

<a id="SKILL-11"></a>
### SKILL-11: a notice is said when it is learned, not when the turn ends

What loaded and what did not is known before the first request goes out, and that is when it is
said. A turn that fails, is cancelled, or never reaches an answer has said it already.

**Why.** A notice describes what the turn is about to work with. Held until the turn is over it
arrives after every tool line, reading as the last thing that happened rather than the first, and
a turn with no outcome carried none at all: the run where a missing skill mattered most was the
run that said nothing about it.

`verified-by: bravebot_agent::turn::what_did_not_load_reaches_the_interface_when_it_is_learned`
`verified-by: bravebot_agent::turn::what_did_not_load_is_reported_even_when_the_turn_never_finishes`

<a id="SKILL-12"></a>
### SKILL-12: a built-in skill is the program's own text, and is the least specific source

Some skills are written into this program rather than found anywhere. There is no file, no
directory and nobody to have vouched for one, so a built-in passes no trust gate, can never be
skipped, and never produces a notice. It is offered in every session, including one in a
directory nobody trusts and one with no home directory at all.

Built-ins are added before any source is read, so a skill of the user's own with the same name
shadows one, which is the same "most specific wins" every other source follows.

**Why this is different in kind.** A name read out of a directory is content: it comes from
whoever wrote the directory, it may not be trusted, and that is why an untrusted skill is counted
rather than named. A name written here comes from this repository, cannot be added to or changed
by anything a turn does, and is the only kind of skill name that may also be a word the interface
itself claims.

`verified-by: bravebot_agent::skills::a_built_in_skill_is_offered_wherever_a_session_runs`
`verified-by: bravebot_agent::skills::a_skill_of_the_users_own_shadows_a_built_in_of_the_same_name`

<a id="SKILL-13"></a>
### SKILL-13: a built-in's description names the condition it is about, and nothing wider

A built-in is offered in every session, per [SKILL-12](#SKILL-12), so nothing else decides when it
applies: the sentence in its description is the whole of that decision, and every condition it names
is an invitation to load the body somewhere. The loop skill's description therefore says to load it
when this turn is a tick of a loop, and says nothing about being asked to watch or to repeat
something.

**Why that second condition was the bug and not a convenience.** The body describes a tick, where an
interval supplies the repetition and the comparison is against what the last tick reported. Read in a
session that is not a tick, the same words are the whole of the turn: look once, report what is there,
and stop, which is exactly the snapshot that leaves nothing watching. It also tells the turn how to
pace the next tick, using a tool that is in the table only when the turn is a tick, so a session that
followed it there would promise a further look that nothing could arrange. A description that
advertises a body to a session the body does not fit is a defect in the description.

**What this clause does not do.** It narrows what the planner is invited to load, not what it may
load: a session that asks for this skill by name still gets it. Keeping the built-in out of the
advertised set altogether, where the turn is not a tick, is a wider change than a sentence and is not
settled here.

`verified-by: bravebot_agent::skills::the_loop_skill_is_advertised_for_a_tick_and_for_nothing_else`
`verified-by: bravebot_agent::skills::the_loop_skill_description_names_one_condition_and_ends_with_it`

<a id="SKILL-17"></a>
### SKILL-17: the documentation is a built-in skill whose pages load one at a time

The published documentation under `docs/website/docs` is compiled into the program as the built-in
skill `bravebot-help`, so a session answers a question about using bravebot from the version that
is running and not from the planner's memory. Its description names one condition, per
[SKILL-13](#SKILL-13): the user asks a question about using bravebot itself.

The skill's body is an index with one line per page. A page is loaded with `load_skill` by the name
on its line, `bravebot-help/<place of the page>`, and a question costs the pages it needs and not the
whole site. The name is matched exactly against the table compiled into the program, so it is never
a path ([SKILL-8](#SKILL-8)), and a page is trusted for the reason the skill is: it is this
program's own text ([SKILL-12](#SKILL-12)). A skill of the user's own named `bravebot-help` shadows
the built-in, and the pages go with it.

The clause headings of `docs/specs/` are carried the same way. The index has one more line, for the
page `bravebot-help/specs`, which lists every spec with its title and the number of its clauses. The
page of one spec is `bravebot-help/specs/<place of the spec file>`, such as
`bravebot-help/specs/tools/read-file`, and holds that spec's file name and the heading of each of
its clauses in file order, each starting with its clause id. The clause text is not carried, so a
question the headings do not settle is answered with the clause id and the file. A page says when
its spec is `proposed`.

The headings are read out of the spec files when the program is built, so a spec that changes is
carried by the next build and no copy of them is kept in the tree. A heading is a `###` line outside
a code fence that starts with a clause id. A file in `docs/specs/` other than its README that has no
frontmatter `id` fails the build rather than dropping out of the skill. A spec page is selected the
way a documentation page is, by an exact match against the compiled table, so the name is never a
path.

[CMD-24](commands.md#CMD-24) is the command that asks for the skill by name. Without it the planner
decides from the description when to load the skill.

**What this clause does not do.** The text of a clause is not carried: a question about the exact
wording of a rule is answered from its heading, and the program holds the clause text nowhere.

`verified-by: bravebot_agent::skills::the_help_skill_is_offered_everywhere_for_a_question_about_bravebot_and_nothing_else`
`verified-by: bravebot_agent::help::every_published_page_is_carried_in_the_binary`
`verified-by: bravebot_agent::help::every_spec_is_carried_with_the_clause_headings_its_file_holds`
`verified-by: bravebot_agent::help::a_spec_page_holds_the_heading_of_a_clause_and_names_its_file`
`verified-by: bravebot_agent::help::a_spec_page_is_found_by_its_exact_name_and_nothing_else`
`verified-by: bravebot_agent::help::the_list_of_specs_names_a_page_that_loads_for_each_spec`
`verified-by: bravebot_agent::help::the_index_names_the_list_of_specs_and_carries_none_of_their_headings`
`verified-by: bravebot_agent::spec_headings::only_a_clause_heading_outside_a_code_fence_is_a_heading`
`verified-by: bravebot_agent::spec_headings::a_fence_is_closed_only_by_the_same_mark_at_least_as_long`
`verified-by: bravebot_agent::spec_headings::a_page_names_its_file_and_says_when_the_spec_is_only_proposed`
`verified-by: bravebot_agent::tools::a_spec_page_loads_by_the_name_the_list_gives_it`
`verified-by: bravebot_agent::tools::a_documentation_page_loads_by_the_name_the_index_gives_it`
`verified-by: bravebot_agent::tools::a_users_skill_of_the_help_name_takes_the_pages_with_it`

<a id="SKILL-14"></a>
### SKILL-14: an interface listing skills is shown the set a turn would advertise, read the same way

Where the interface lists skills for a person, it reads them through the same gate a turn does,
under the session's trust map and its rules, so a skill a turn would drop is never listed and one a
turn would offer is. Each skill records whether it was found in the project, in the person's own
directory (`~/.bravebot/skills` or a shared one they named, [INSTR-17](instructions.md#INSTR-17))
or in this program, and one that shadowed another records where it was found, not where
the one it shadowed was.

The list draws names, descriptions and argument hints and loads no skill, so it does not list the
files beside each skill. Everything else it reads is what a turn reads.

A skill kept from the planner ([SKILL-20](#SKILL-20)) is in the list although no turn advertises it,
and records that it is, so the list can say that only the person can have it loaded. It is the one
skill the list holds that a turn's planner is not told of, and the list is how a person finds the name
to type.

**Why.** A list that found skills its own way would sooner or later name one the planner is never
shown, and a skill in a project nobody trusts is exactly the one [SKILL-6](#SKILL-6) counts rather
than names. Where it was found is what lets a person tell a skill of their own from one a checkout
brought with it. Walking every skill's directory for files only `load_skill` reads on each slash
would make the list lag behind the keys.

`verified-by: bravebot_agent::skills::the_set_an_interface_resolves_is_the_one_a_turn_would`
`verified-by: bravebot_agent::skills::the_set_the_box_offers_is_the_turns_without_the_files_beside_each_skill`
`verified-by: bravebot_agent::skills::each_skill_records_which_of_the_three_places_it_came_from`
`verified-by: bravebot_agent::skills::a_skill_a_deny_rule_covers_is_offered_nowhere_through_a_link_to_it`
`verified-by: bravebot_agent::skills::the_set_an_interface_resolves_still_holds_a_skill_kept_from_the_planner`

<a id="SKILL-15"></a>
### SKILL-15: a skill may name the model its rounds are asked of and the effort they carry

`model` and `effort` beyond the name and the description, both optional. `model` is resolved the way
the settings key's value is, aliases included, so `haiku` names a tier here as it does there.
`effort` is one of the five levels, read without regard to case. Absence in either leaves the
session's own choice in force, and a blank value is absence: a line somebody started and did not
finish names nothing.

Both take effect from the round after `load_skill` answers, and for the rest of the turn. The rounds
before the call are asked at the session's, the skill not having been in force yet. A ChatGPT account is
the exception for `effort`: it is sent the level its session began with whatever the skill names
([CHATGPT-10](chatgpt-sign-in.md#CHATGPT-10)).

**The skill's model replaces the session's, including a model the person picked with `/model`, and
the switch is announced** per [SKILL-11](#SKILL-11). A skill asking for a cheaper model than the
session's is the main use of the key, and it would do nothing if the session's choice won. The cost
is that a skill may ask for a dearer model than the person picked, and the notice is how they learn
of it.

**A definition's model is not replaced.** Where the turn runs on a model that an addressed
definition ([ADDRESS-11](addressing-a-definition.md#ADDRESS-11)) or the delegate's own definition
([DELEGATE-22](delegation.md#DELEGATE-22)) named, a skill loaded in that turn leaves it in force, and
the notice names the skill, the model it asked for, and the definition. Both specs treat that model
as a cost boundary the definition's file drew, and a skill the planner loads during the turn is not
a choice the person made. The skill's `effort` still applies, since a definition names none.
Where `--model` was asked for in place of the addressed definition's model
([ADDRESS-11](addressing-a-definition.md#ADDRESS-11)), the turn does not run on the definition's
model, so a skill's model replaces the command line's, as it would with no definition addressed.

**A skill's model is compared with the model that answered.** The endpoint substitutes a model it
will not serve rather than refusing it, so where the rounds after a switch are answered by another
model, the turn says so, naming the skill and the model as its file wrote it. This holds in a
delegate's turn too. The front end does not compare that answer with the session's own model, which
those rounds did not ask for, and a one-shot run whose command line named a model does not fail
over it.

A delegate spawned after a skill switched the model is lent the model in force when it started,
unless its own definition names one.

`verified-by: bravebot_agent::skills::a_skill_reads_the_model_and_the_effort_it_names`
`verified-by: bravebot_agent::skills::a_model_that_names_nothing_leaves_the_session_its_own`
`verified-by: bravebot_agent::skills::a_skill_carries_the_model_and_the_effort_its_file_named`
`verified-by: bravebot_agent::turn::a_loaded_skill_asks_the_rounds_after_it_of_its_own_model_and_effort`
`verified-by: bravebot_agent::turn::a_delegate_spawned_after_a_skill_switched_the_model_is_asked_of_that_model`
`verified-by: bravebot_agent::turn::a_skill_naming_neither_key_leaves_the_session_its_own_choice`
`verified-by: bravebot_agent::turn::a_skill_loaded_by_an_addressed_definition_keeps_the_definitions_model`
`verified-by: bravebot_agent::turn::a_skill_under_a_definition_the_command_line_outranked_switches_the_model`
`verified-by: bravebot_agent::turn::a_skill_loaded_by_a_delegate_keeps_its_definitions_model`
`verified-by: bravebot_agent::turn::a_skill_answered_by_its_own_model_is_not_reported_as_a_substitution`
`verified-by: bravebot_agent::turn::a_skill_answered_by_a_model_other_than_its_own_says_so`
`verified-by: bravebot_agent::turn::a_skill_loaded_by_a_delegate_and_answered_by_another_model_says_so`

<a id="SKILL-16"></a>
### SKILL-16: a value that cannot be used is reported and the skill still loads

An `effort` naming none of the five levels, and a `model` needing a sign-in this machine has not
made, are each said and the skill is offered anyway, on whatever the session was already running.
The same holds for a word [SKILL-20](#SKILL-20) cannot read, except that the skill is then kept from
the planner rather than offered to it.
The notice names the file and the value as the file wrote it, both of which are the words of a source
somebody vouched for.

**Why this is not a skip.** A file missing its name or its description is dropped, per
[SKILL-1](#SKILL-1), because the planner chooses from those two and a file offering neither is
unchoosable. Neither of these is: a skill that named no model at all is choosable, so dropping one
over a misspelt level would take a whole set of instructions away over a line that was only ever an
adjustment to how they run. It is also not a refused turn, which is what an addressed definition
naming an unreachable model gets: a skill is not the thing the person asked for.

**A key nothing reads is reported by `doctor`.** Not in the session. Almost every skill written for
another agent carries one, and a line repeated every turn about something that is working is how a
notice stops being read; but a key dropped in silence is a line whose author believes it is in force,
which is how the next field added becomes another quiet no-op. `doctor` reads the set the way a turn
would, so no skill a turn would drop is named there, and says nothing about a skill whose every key
is read. That includes a skill from a shared directory the person named
([INSTR-17](instructions.md#INSTR-17)). `doctor` also names a shared directory that exists and is not
named, in the same section, since a person who keeps skills for another agent would otherwise not
learn that this program found the directory and left it alone.

`verified-by: bravebot_agent::skills::an_effort_word_that_names_no_level_leaves_the_session_its_own`
`verified-by: bravebot_agent::skills::an_effort_word_naming_no_level_is_reported_and_the_skill_still_loads`
`verified-by: bravebot_agent::skills::a_key_nothing_reads_is_carried_out_beside_the_skill_that_declared_it`
`verified-by: bravebot_cli::running::doctor_names_a_skill_key_nothing_reads`
`verified-by: bravebot_cli::running::doctor_says_nothing_of_a_skill_with_every_key_read_or_one_a_turn_drops`
`verified-by: bravebot_cli::running::doctor_names_a_projects_unread_key_only_where_the_directory_is_trusted`
`verified-by: bravebot_cli::running::doctor_names_a_shared_skill_directory_it_left_alone`
`verified-by: bravebot_cli::running::doctor_reads_a_named_shared_skill_directory_and_names_an_unread_key_in_it`
`verified-by: bravebot_agent::turn::a_skill_whose_model_needs_a_sign_in_keeps_the_sessions_model_and_says_so`

<a id="SKILL-18"></a>
### SKILL-18: a skill of the user's own is found with `search_skills`, not listed by default

Only a built-in ([SKILL-12](#SKILL-12)) is in the prompt's listing by default. A skill of the
user's own or the project's is not, however many there are: it is found by calling
[`search_skills`](tools/search-skills.md) with a word or phrase, which matches it by name or
description against the whole catalogue, built-ins included, and returns each match in full. A
skill `search_skills` finds is loaded by `load_skill` the same way a listed one is
([LOAD-1](tools/load-skill.md#LOAD-1)): a skill search finds is never a skill `load_skill` then
refuses. A skill kept from the planner ([SKILL-20](#SKILL-20)) is neither found nor loaded by a
call.

A definition's own `skills:` line ([ADDRESS-1](addressing-a-definition.md#ADDRESS-1),
[DELEGATE-23](delegation.md#DELEGATE-23)) is the one exception: naming skills there already makes
a short, deliberate list, so the whole of it is listed directly rather than held back for
`search_skills` to find. A definition naming none keeps the ordinary default.

No description is ever cut, wherever one reaches the planner: not in the default listing, not in
a `search_skills` result. One past 1,024 characters is said as a count instead, never cut and
never named, per [SKILL-6](#SKILL-6). The count is said to the person once per session rather than
once per turn, the way [SKILL-19](#SKILL-19) says the overview is: a long description does not
change turn to turn, so there is nothing a second telling would add.

**Why.** The name and description of every skill in a directory going into every request leaves a
directory of fifty verbose skills less room for the task than the task needs, and the body each
skill keeps out of the prompt ([SKILL-2](#SKILL-2)) bounds nothing about that. Holding the default
listing to built-ins removes the cost at its source, and a search call that returns every
description whole costs only the one request that asks for it. Cutting a description to fit would
risk losing the half of it that mattered: a person who wrote that much meant to, and the cost is
worth a count rather than a guess at which half was safe to drop.

`verified-by: bravebot_agent::skills::describe_for_prompt_lists_only_built_in_skills`
`verified-by: bravebot_agent::skills::a_long_description_is_never_cut_and_is_counted`
`verified-by: bravebot_agent::skills::search_matches_every_word_case_insensitively_and_caps_the_result`
`verified-by: bravebot_agent::turn::a_skill_body_stays_out_of_the_context_until_search_finds_it_and_load_skill_is_asked`
`verified-by: bravebot_agent::turn::a_long_description_is_said_as_a_count_and_never_cut`
`verified-by: bravebot_agent::turn::a_long_description_warning_is_said_once_per_session_not_once_per_turn`
`verified-by: bravebot_agent::turn::an_addressed_turn_is_offered_only_the_skills_its_definition_names`
`verified-by: bravebot_agent::preamble::the_skills_section_points_at_search_skills_only_where_more_could_be_found`

<a id="SKILL-19"></a>
### SKILL-19: the first turn of a session not already told puts the count and names of what the listing leaves out in the conversation

Where [SKILL-18](#SKILL-18) holds a skill back from the default listing, the first turn of a
session that has not already been told puts a message in the conversation giving how many such
skills there are and what each is named, so the planner can tell whether `search_skills` could
find anything before it calls it. A message in the conversation rather than a line in the system
prompt: the prompt is rebuilt fresh every turn and none of it is kept, so a planner told only
there would have the count and the names for the one turn that built it and nothing of its own to
have remembered either by. Put in the conversation once, the count and the names are carried
forward in every later request the ordinary way any earlier exchange is, and the turn that put
them there does not put them there again.

The message is a user message after the prompt, and a wire that merges consecutive user messages
into one turn starts its text on a new paragraph ([BACKEND-57](backends.md#BACKEND-57)), so the
count it begins with is never read as the end of a prompt that finishes in a number.

Names only, never a description: a description is what `search_skills` is for, and giving every
one away up front would pay back the cost [SKILL-18](#SKILL-18) removed it to avoid. Nothing is
put there where there is nothing to report, so a curated catalogue ([DELEGATE-23](delegation.md#DELEGATE-23))
and a session with no skill beyond a built-in are both silent on it, and a session that starts
silent because of the second reason is not thereby marked told: a skill written into the project
later in the session still gets its own turn to be named.

**Why.** Without the message, a planner that never thinks to call `search_skills` never learns an
unlisted skill exists. A count and names cost one message, once per session, against the room a
full listing would cost on every turn of it.

`verified-by: bravebot_agent::skills::other_skill_names_is_empty_for_a_curated_catalogue`
`verified-by: bravebot_agent::turn::the_skills_overview_is_said_once_per_session_not_once_per_turn`
`verified-by: bravebot_agent::turn::an_addressed_turn_is_offered_only_the_skills_its_definition_names`
`verified-by: bravebot_agent::turn::a_skill_written_in_after_an_empty_first_turn_is_still_named_on_the_next`
`verified-by: bravebot_agent::turn::the_skills_overview_survives_a_prompt_handed_back_to_the_box`
`verified-by: bravebot_agent::turn::the_skills_overview_is_a_separate_paragraph_from_the_prompt_on_block_based_wires`

<a id="SKILL-20"></a>
### SKILL-20: `disable-model-invocation: true` keeps a skill from the planner, and a person's `/name` loads it

`disable-model-invocation` is read as a sixth key. `true`, in any case, keeps the skill from the
planner; `false`, a blank value and no key leave the skill as it was. A trailing `# comment` is not
part of the value. A word that is neither is read as `true` and said back, naming the file and the
word as it wrote it, per [SKILL-16](#SKILL-16): the key exists to keep a skill with side effects from
being chosen by a model, so a misspelling that left the skill advertised would expose the skill the
key was written to protect, and the person can still name it.

**Not advertised.** Neither the name nor the description of such a skill is in the system prompt, and
the prompt has no section for skills where the catalogue holds no other. `doctor` does not report the
key as one nothing reads.

**Loaded by the person.** A turn whose prompt a person typed names a skill kept from the planner when
a word of that prompt is `/name`, once the quotes, brackets and sentence punctuation around it are
taken off, matched exactly against the names found before the turn
([LOAD-2](tools/load-skill.md#LOAD-2)): `/name.` and `(/name)` name the skill, and `/Name`, a prefix,
a longer name and the bare name select nothing.
The driver then puts the skill's instructions in front of the planner as a message after the line
that named it, shown through `Policy::present` with the label they were read with, and no
`load_skill` call is made. Where the gate would not show them, the turn stops with a sentence naming
the skill rather than going on without it, and nothing is recorded as supplied. The result carries
the files beside the skill and the reach into its directory that `load_skill`'s does
([LOAD-4](tools/load-skill.md#LOAD-4), [LOAD-5](tools/load-skill.md#LOAD-5)). The skill's `model`
and `effort` apply from the first round, since it is in force before the first request goes out
([SKILL-15](#SKILL-15)), and a model an addressed definition named stays in force as it does for a
skill the planner loads. The trail records the skill by name with the size of what went, and the
person is told it was loaded, since no tool call is drawn for it ([SKILL-11](#SKILL-11)).

**Under a definition.** An addressed definition's `skills` line limits what its planner may load. It
does not stop a skill the person names in their own line, which is supplied as in any other turn.

**Refused always.** `load_skill` selects from the set a turn found and never selects a skill kept
from the planner, whether or not the person named it in that turn: the instructions were supplied for
the line, so a call is answered as for a name that matches nothing and says nothing about a skill the
planner was never told of. A delegate is offered none. The prompt of a delegate's task, a prompt the
driver composed and a name inside a file or a tool result name nothing. The line is the person's own
prompt and the names are the driver's, so nothing a file or a tool said decides what is supplied.

**Why.** A skill for a step with side effects, such as a release or a deploy, should run only when a
person asks. The planner chooses from the descriptions it is shown, and a `/name` in a prompt is
words the planner may honour or not ([CMD-9](commands.md#CMD-9)), so neither keeps a skill to the
person. The driver compares the person's typed word with names it found before the turn, as it
compares an `@` mention, and does not branch on untrusted content. A description of a skill the
planner should never choose is also context spent on it every turn. Other coding agents document the
same key under the same name.

**What it does not do.** The costs of this design are listed under Known costs: a line typed while a
turn runs, the files beside a skill in a later turn, a loop's repeated line, a name inside a line that
says not to run it, and a name that cannot be typed. A definition's
`skills` line that names such a skill offers it to no delegate, and says so when the session starts.

`verified-by: bravebot_agent::skills::only_a_true_value_keeps_a_skill_from_the_planner`
`verified-by: bravebot_agent::skills::a_value_that_is_neither_true_nor_false_keeps_the_skill_from_the_planner_and_is_reported`
`verified-by: bravebot_agent::skills::the_key_is_read_and_not_carried_out_as_unread`
`verified-by: bravebot_cli::running::doctor_says_nothing_of_a_skill_with_every_key_read_or_one_a_turn_drops`
`verified-by: bravebot_agent::skills::a_skill_kept_from_the_planner_is_left_out_of_what_the_prompt_advertises`
`verified-by: bravebot_agent::skills::a_catalogue_of_kept_skills_alone_advertises_nothing`
`verified-by: bravebot_agent::preamble::a_catalogue_holding_only_a_skill_kept_from_the_planner_has_no_section_of_skills`
`verified-by: bravebot_agent::skills::a_line_names_a_kept_skill_only_by_its_whole_name_after_a_slash`
`verified-by: bravebot_agent::skills::a_trailing_comment_is_not_part_of_the_value`
`verified-by: bravebot_agent::turn::a_skill_the_person_named_cannot_also_be_loaded_by_the_planner`
`verified-by: bravebot_agent::turn::every_skill_a_line_names_is_supplied_and_no_other`
`verified-by: bravebot_agent::turn::a_skill_the_person_names_is_supplied_under_a_definition_that_does_not_list_it`
`verified-by: bravebot_agent::agents::a_definition_naming_a_skill_kept_from_the_planner_is_told_it_is_offered_none`
`verified-by: bravebot_agent::turn::a_skill_the_person_names_is_supplied_to_the_planner_without_a_load_call`
`verified-by: bravebot_agent::turn::a_skill_the_person_names_asks_its_model_and_effort_from_the_first_round`
`verified-by: bravebot_agent::turn::a_skill_kept_from_the_planner_is_unreachable_in_a_turn_that_does_not_name_it`
`verified-by: bravebot_agent::turn::text_the_person_did_not_type_names_no_skill`
`verified-by: bravebot_agent::turn::a_delegate_is_given_no_skill_that_is_kept_from_the_planner`
`verified-by: bravebot_agent::turn::supplying_a_named_skill_of_the_users_own_opens_its_directory_for_reading`
`verified-by: bravebot_agent::turn::a_named_skill_the_gate_holds_back_stops_the_turn_and_supplies_nothing`
`verified-by: bravebot_agent::conversation::a_skill_the_person_named_is_sent_again_word_for_word_after_a_cut`
`verified-by: bravebot_agent::conversation::the_cut_of_a_skill_says_who_can_read_the_rest`
`verified-by: bravebot_agent::conversation::a_cut_never_falls_in_front_of_a_skill_the_person_named`

<a id="SKILL-21"></a>
### SKILL-21: a skill a person put in force as a mode is in the system prompt of every turn until they leave it

`/mode <skill>` ([CMD-27](commands.md#CMD-27)) names one skill as the session's mode and `/mode off`
leaves it. A turn built while a mode stands reads the skill again when it begins, from the set it
found ([LOAD-2](tools/load-skill.md#LOAD-2)), by the whole name and no other comparison, and puts its
instructions in the system prompt: after what this program says and ahead of the person's own
`AGENTS.md`, so those words still have the last word over a skill. The words go through
`Policy::present` with the label the skill was read with, as a skill the planner loads does, and the
files beside the skill are reachable as they are for one a person names ([SKILL-20](#SKILL-20)).
They are not a message in the conversation, so a session of many turns holds them once, and nothing
a compaction keeps ([COMPACT-17](compaction.md#COMPACT-17)) has to carry them. A skill its file
keeps from the planner can be a mode, since the person chose it, and is still not advertised to the
planner. The trail records the skill by name with the size of what went, each turn.

A mode that cannot be applied to a turn leaves the turn without it and says so in the turn's notices:
a skill that is no longer found, a skill of a project the trust map no longer vouches for, a skill
the gate would hold back, and a skill `permissions` denies reading. Nothing falls back to another skill.
Nothing a file or a tool said sets or clears a mode, and `load_skill` cannot: the mode is a name the
person's own command stored, compared whole against names the driver enumerated. A delegate is
given none, and neither is a turn whose caller is not the terminal interface.

The record stores the name, so a resume puts the mode back and says that it did, and a fork or a
`/clear` keeps it. A record from before the mode was kept reads as in none. The skill's `model`
and `effort` are not applied by a mode: only its instructions are ([SKILL-15](#SKILL-15)).

**Why.** A skill that says how to work, a review checklist held across several files or a test-first
playbook, is wanted for a span of the session and not for one message, and naming it in each prompt
is what a person forgets to do. Putting it in the system prompt keeps it out of the history, where it
would repeat each turn. Reading it again each turn, through the gate, is what stops a choice made while
a project was vouched for from outliving the vouching.

`verified-by: bravebot_agent::turn::a_skill_mode_is_in_the_system_prompt_of_every_turn_and_only_once`
`verified-by: bravebot_agent::turn::a_skill_mode_that_the_gate_or_the_catalogue_does_not_hold_is_not_applied_and_is_said`
`verified-by: bravebot_session::sessions::a_session_records_the_skill_mode_and_its_fork_and_resume_keep_it`
`verified-by: bravebot_tui::app::a_resumed_session_is_in_the_mode_its_record_names`
`verified-by: bravebot_tui::render::the_box_names_the_mode_while_one_is_in_force`

## Known costs

- **A mode applies a skill's instructions and nothing else of it ([SKILL-21](#SKILL-21)).** Its
  `model` and `effort` are not applied, since they take effect from the round after a load and a mode
  has no load to follow. A person who wants them names the skill in a line as well.

- **A skill kept from the planner is supplied only for a line the person sent as a prompt.** A
  `/name` typed while a turn runs is an interjection, which reaches the planner as words and brings
  no skill ([SKILL-20](#SKILL-20)). The planner may call `load_skill` for it and be refused, and the
  person sends the name in a line of their own once the turn ends. Supplying a skill in the middle of
  a turn would have to apply its model and effort to rounds already planned.

- **The files beside a skill of the person's own are readable for the turn that named it.** The
  reach into the directory belongs to the turn ([LOAD-5](tools/load-skill.md#LOAD-5)), and
  `load_skill` cannot be called to open it again for a skill kept from the planner. A later turn that
  needs a file beside it asks the person to name the skill again. A project's skill has no such limit,
  since the trust map decides each of its files.

- **A kept skill whose name holds a space or a control character is never loaded.** No word of a
  line can equal such a name, so the key leaves the skill with nobody able to load it, and the box
  does not offer it either ([CMD-9](commands.md#CMD-9)).

- **Each turn that names the skill supplies it again.** A loop resends the line the person typed, so
  one that names a skill carries a copy of its instructions for every tick until a compaction sends
  the newest alone ([COMPACT-17](compaction.md#COMPACT-17)).

- **A `/name` anywhere in the line names the skill, including a line that says not to run it and
  text the person pasted into it.** The driver compares words with names and reads no sentence, since
  reading what the line means is the judgment it does not make, and a pasted word is not told apart
  from a typed one. The instructions are introduced with "Apply them as that line asks",
  and the person is told the skill was loaded, so the planner can follow a line that refuses it and
  the person sees what happened.

- **A skill downloaded into `~/.bravebot/skills` is trusted exactly as far as a config file the
  user pasted is.** The name, the description and the body all go to the model as instructions.
  Nothing downstream second-guesses it, because everything downstream is built to trust what the
  user vouched for. Read one before installing it.

- **A skill of the user's own or the project's is loaded only where the planner calls
  `search_skills` for it, where a built-in is seen whether the planner looks for it or not.**
  [SKILL-19](#SKILL-19) names it once, on the first turn a session has one to name, but a skill
  written into the project after that turn has no later one: the names said were the ones found
  then, and nothing says the count changed.

- **The premium key is handed to a turn once, from the model the turn starts on, so a skill that
  moves the rounds onto a model of Brave's finds none in hand.** Such a round is answered by a
  weaker model than the file asked for, and the turn reports the substitution per
  [SKILL-15](#SKILL-15). It happens only where the model the turn started on is served by another
  backend and the one a skill names is Brave's. Handing the key to every turn because some skill in
  the directory names a premium model would hand it to turns on other backends, which
  [PREM-13](premium-credentials.md#PREM-13) forbids.
