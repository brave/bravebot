---
id: FINDSKILL
title: search_skills
status: normative
governs:
  - crates/agent/src/skills.rs
documented-by:
  - docs/website/docs/reference/tools.md
  - docs/website/docs/customize/skills.md
---

## Scope

Finding a skill of the user's own or the project's that [SKILL-17](../skills.md#SKILL-17) left
out of the default listing. `query` is routing; there are no content arguments. The result is a
match's name and description, in full, the same way the default listing shows a built-in's. Where
skills come from and what they are trusted for is [skills.md](../skills.md); loading a match by
name is [load_skill](load-skill.md).

## Clauses

<a id="FINDSKILL-1"></a>
### FINDSKILL-1: a skill matches where every word of the query is in its name or its description

The query is split on whitespace, lowercased, and every piece has to be found, in either the
name or the description, for the skill to match. One word that is nowhere in either leaves the
skill out, so a longer query narrows what matches rather than widening it. A blank query matches
nothing.

**Why.** A single comparison that a query of several words either all pass or none do is
easier to reason about than a query language, and the planner is better served by a query it can
simply add a word to than one it has to escape or quote.

`verified-by: bravebot_agent::skills::search_matches_every_word_case_insensitively_and_caps_the_result`

<a id="FINDSKILL-2"></a>
### FINDSKILL-2: the search runs over every skill the turn found, not only the ones listed

A built-in, a skill of the user's own and a project's own are all searched, whether or not
[SKILL-17](../skills.md#SKILL-17) lists any of them by default. Every entry here already passed
the trust gate [SKILL-5](../skills.md#SKILL-5) requires before the catalogue held it, so matching
one reads no untrusted content and nothing a directory wrote decides what matches: only the
length and the position of a word the planner typed against a name and a description the trust
gate already cleared.

A definition's own `skills:` line narrows this the same way it narrows the listing
([SKILL-17](../skills.md#SKILL-17), [DELEGATE-23](../delegation.md#DELEGATE-23)): the search runs
only over the names it kept, so a skill it left out is unreachable by this tool as well as absent
from the prompt.

`verified-by: bravebot_agent::turn::a_skill_body_stays_out_of_the_context_until_search_finds_it_and_load_skill_is_asked`
`verified-by: bravebot_agent::skills::a_curated_catalogues_search_does_not_reach_a_skill_outside_its_list`

<a id="FINDSKILL-3"></a>
### FINDSKILL-3: a match's description is returned in full, never cut

Past 1,024 characters a description is long enough that the person is told so, as a count, per
[SKILL-17](../skills.md#SKILL-17), but a match's description is never shortened to fit the
result.

`verified-by: bravebot_agent::skills::a_long_description_is_never_cut_and_is_counted`
`verified-by: bravebot_agent::turn::a_long_description_is_said_as_a_count_and_never_cut`

<a id="FINDSKILL-4"></a>
### FINDSKILL-4: a broad query returns at most 25 matches, and says how many more there were

Matches are returned in [SKILL-2](../skills.md#SKILL-2) order, capped at 25, with a trailing line
giving the number past the cap. Nothing here is named: the count is a reason to add a word to the
query, not a list of what the query already found too much of.

**Why.** A query of one common word over a large directory could otherwise return the whole
catalogue in a single call, which is the cost this tool exists to avoid paying by default.

`verified-by: bravebot_agent::skills::search_matches_every_word_case_insensitively_and_caps_the_result`
`verified-by: bravebot_agent::tools::search_skills_says_how_many_more_matched_past_the_cap`
