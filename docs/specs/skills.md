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
[SKILL-15](#SKILL-15) is the two keys read for how the skill runs. `argument-hint` is read too: an
interface draws it after the skill's name to say what the skill takes ([commands.md](commands.md)),
and it is never given to the planner. A file with no frontmatter is not a skill.

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
### SKILL-2: only the name and description reach the prompt; the body waits for `load_skill`

The description is what the planner decides from, so it should say *when* to use the skill rather
than what it contains.

**Why.** A directory of long skills would otherwise crowd out the task.

`verified-by: bravebot_agent::skills::what_the_prompt_advertises_holds_no_bodies`
`verified-by: bravebot_agent::turn::a_skill_body_stays_out_of_the_context_until_it_is_asked_for`
`verified-by: bravebot_agent::skills::skills_are_offered_in_the_same_order_every_time`

<a id="SKILL-3"></a>
### SKILL-3: `~/.bravebot` is trusted by provenance, never by the trust map

It is labelled from provenance, because the trust map is keyed by workspace-relative paths and has
nothing to say about a path outside the workspace. Never label a workspace path this way: that
would be laundering.

Nothing is assumed from silence. An empty directory offers nothing, and putting a file there is
the grant, on the same footing as the configuration that picks the model and the endpoint.

`verified-by: bravebot_agent::skills::a_home_skill_is_not_labelled_by_a_rule_meant_for_the_workspace`

<a id="SKILL-4"></a>
### SKILL-4: a project's own files are read through the trust map

A workspace `AGENTS.md` and a project's skill directories are labelled as workspace content, so
TRUST decides. Each skill directory is checked for trust **before it is enumerated at all**,
because a directory name is content too. Which directories those are, and in what order, is
[INSTR-12](instructions.md#INSTR-12).

`verified-by: bravebot_agent::skills::a_skill_in_an_untrusted_project_is_not_named_to_the_planner`
`verified-by: bravebot_agent::skills::a_skill_the_trust_map_distrusts_stops_being_offered`
`verified-by: bravebot_agent::skills::a_foreign_skill_in_an_untrusted_project_is_counted_and_not_named`

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

<a id="SKILL-14"></a>
### SKILL-14: an interface listing skills is shown the set a turn would advertise, read the same way

Where the interface lists skills for a person, it reads them through the same gate a turn does,
under the session's trust map and its rules, so a skill a turn would drop is never listed and one a
turn would offer is. Each skill records whether it was found in the project, in the person's own
directory or in this program, and one that shadowed another records where it was found, not where
the one it shadowed was.

**Why.** A list that found skills its own way would sooner or later name one the planner is never
shown, and a skill in a project nobody trusts is exactly the one [SKILL-6](#SKILL-6) counts rather
than names. Where it was found is what lets a person tell a skill of their own from one a checkout
brought with it.

`verified-by: bravebot_agent::skills::the_set_an_interface_resolves_is_the_one_a_turn_would`
`verified-by: bravebot_agent::skills::each_skill_records_which_of_the_three_places_it_came_from`
`verified-by: bravebot_agent::skills::a_skill_a_deny_rule_covers_is_offered_nowhere_through_a_link_to_it`

<a id="SKILL-15"></a>
### SKILL-15: a skill may name the model its rounds are asked of and the effort they carry

`model` and `effort` beyond the name and the description, both optional. `model` is resolved the way
the settings key's value is, aliases included, so `haiku` names a tier here as it does there.
`effort` is one of the five levels, read without regard to case. Absence in either leaves the
session's own choice in force, and a blank value is absence: a line somebody started and did not
finish names nothing.

Both take effect from the round after `load_skill` answers, and for the rest of the turn. The rounds
before the call are asked at the session's, the skill not having been in force yet.

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
is read.

`verified-by: bravebot_agent::skills::an_effort_word_that_names_no_level_leaves_the_session_its_own`
`verified-by: bravebot_agent::skills::an_effort_word_naming_no_level_is_reported_and_the_skill_still_loads`
`verified-by: bravebot_agent::skills::a_key_nothing_reads_is_carried_out_beside_the_skill_that_declared_it`
`verified-by: bravebot_cli::running::doctor_names_a_skill_key_nothing_reads`
`verified-by: bravebot_agent::turn::a_skill_whose_model_needs_a_sign_in_keeps_the_sessions_model_and_says_so`

## Known costs

- **A skill downloaded into `~/.bravebot/skills` is trusted exactly as far as a config file the
  user pasted is.** The name, the description and the body all go to the model as instructions.
  Nothing downstream second-guesses it, because everything downstream is built to trust what the
  user vouched for. Read one before installing it.

- **A subscription is looked for once per turn, from the model the turn starts on, so a skill that
  moves the rounds onto a model needing one finds none in hand.** Such a round is answered by a
  weaker model than the file asked for, and the turn reports the substitution per
  [SKILL-15](#SKILL-15). It happens only where the model the turn started on needs no subscription
  and the one a skill names does. Looking for one up front because some skill in the directory names
  a premium model would read the credential store, and report an unreadable batch, on every turn in
  that project, including the ones that never load it.
