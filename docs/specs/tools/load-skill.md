---
id: LOAD
title: load_skill
status: normative
governs:
  - crates/agent/src/skills.rs
documented-by:
  - docs/website/docs/reference/tools.md
  - docs/website/docs/customize/skills.md
---

## Scope

Fetching the body of a skill named in the system prompt. `name` is routing; there are no content
arguments. The result is the skill's text, followed by the files kept beside it where it keeps any. Where skills come from and what they are trusted for is
[skills.md](../skills.md).

## Clauses

<a id="LOAD-1"></a>
### LOAD-1: the name selects from a set fixed before the turn, and never becomes a path

It is promoted the way a read path is, but is more confined than one: the name never becomes a
path component, it only picks from a set the driver enumerated before the turn began. A name
holding a traversal matches nothing, because there is no lookup for it to reach.

`verified-by: bravebot_agent::turn::loading_a_skill_that_does_not_exist_is_refused_rather_than_guessed`

<a id="LOAD-2"></a>
### LOAD-2: a name close to a real one is refused, not guessed at

The set is matched exactly. A name one character out selects nothing, and nothing falls back to a
prefix, a case-insensitive comparison or a nearest match.

**Why.** Guessing would load instructions nobody asked for.

`verified-by: bravebot_agent::skills::a_name_one_character_off_selects_no_skill`
`verified-by: bravebot_agent::turn::loading_a_skill_that_does_not_exist_is_refused_rather_than_guessed`

<a id="LOAD-3"></a>
### LOAD-3: a body reaches the context only when it is asked for

The system prompt advertises names and descriptions and holds no bodies, which is what keeps a
directory of long skills from crowding out the task.

`verified-by: bravebot_agent::turn::a_skill_body_stays_out_of_the_context_until_it_is_asked_for`
`verified-by: bravebot_agent::skills::what_the_prompt_advertises_holds_no_bodies`

<a id="LOAD-4"></a>
### LOAD-4: the files beside a skill are named from a list fixed before the turn

A skill is a directory, and its `SKILL.md` may point at other material kept next to it. The result
of `load_skill` ends with the directory and the regular files beneath it other than `SKILL.md`, by
name relative to the directory. The list is made when the catalogue is resolved and never again, and
the call selects nothing from it: there is no argument for a file, so nothing read can pick one.

A file is named only where the gate the skill passed would let it be read. A skill of the user's own
is trusted by provenance ([SKILL-3](../skills.md#SKILL-3)), so its files are named unless a `deny`
rule covers one. A project's skill is named file by file through the trust map and the
rules, since a file name is content ([SKILL-4](../skills.md#SKILL-4)). A link is left out whatever it
points at, and the directory is searched where it lands, so a link out of it is never followed and
never named. A name that is not text or holds a control character is left out, and the list is capped
in count and in depth.

**Why.** Without it the planner cannot tell what a skill keeps beside its instructions, and the
author has to fold every reference into the one body, which defeats loading it only when asked.
Running a bundled script is not part of this: `run` keeps its own gate.

`verified-by: bravebot_agent::tools::a_home_skills_sibling_file_is_readable_after_it_is_loaded_and_not_before`
`verified-by: bravebot_agent::tools::the_listing_names_regular_files_inside_the_directory_and_nothing_a_rule_covers`
`verified-by: bravebot_agent::tools::a_project_skill_lists_only_the_files_the_trust_map_vouches_for`

<a id="LOAD-5"></a>
### LOAD-5: loading a skill of the user's own lets its directory be read, and nothing else

After `load_skill` answers for a skill of the user's own, `read_file` of a path inside that skill's
directory is allowed for the rest of the turn, and the file takes its label from provenance rather
than from the trust map, which does not govern `~/.bravebot` ([TRUST-11](../trust-map.md#TRUST-11)).
Before the load, and for a skill nobody loaded, the same path is refused as any path outside the
workspace is.

The reach is for reading, in that directory. A write or an edit there is refused as before, a path
that climbs out of the directory or lands outside it through a link is refused, and a setting that
keeps the file tools inside the project ([PERM-16](../permissions.md#PERM-16)) is not widened by it.
A project's skill is given no reach at all: its files are inside the workspace and the trust map
decides each one as it does any other read.

**Why.** A skill's files in the user's own directory are outside the workspace, and without the
reach the list [LOAD-4](#LOAD-4) names would be files nobody can read. The directory is the driver's,
found before the turn and named by a load that already happened, so nothing a file or a planner said
widens it, and a skill the user put there is trusted on the footing [SKILL-3](../skills.md#SKILL-3)
gives its body.

`verified-by: bravebot_agent::tools::a_home_skills_sibling_file_is_readable_after_it_is_loaded_and_not_before`
`verified-by: bravebot_agent::tools::reaching_a_loaded_skills_files_is_for_reading_inside_the_directory_only`
`verified-by: bravebot_agent::tools::a_load_does_not_widen_a_session_kept_inside_its_project`
`verified-by: bravebot_agent::tools::a_project_skill_lists_only_the_files_the_trust_map_vouches_for`
