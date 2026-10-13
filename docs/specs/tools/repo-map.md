---
id: MAP
title: repo_map
status: normative
governs:
  - crates/agent/src/repo_map.rs
  - crates/agent/src/workspace.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

A ranked map of the declarations in a directory's source files, for a planner that has no language
server to ask ([lsp.md](lsp.md)). `directory` and `budget` are routing; there are no content
arguments. The result is the map text, with a count of what it left out.

## Clauses

<a id="MAP-1"></a>
### MAP-1: only files the trust map vouches for are opened, and the others are counted and never named

Which files are vouched for is a question about the trust map, answered from the path alone, so no
byte of an unvouched file is read and none can steer what is read. The map is therefore trusted
whenever it is returned. What it says of the files it left out is a number.

**Why.** A map is text the planner reads, built from many files. Opening an unvouched file to decide
whether to show it lets that file choose what the planner sees, and a name or a declaration is
enough to carry an instruction. [LIST-1](list-files.md#LIST-1) quarantines a listing for the same
reason, but a listing can hand over a reference per entry and a map cannot, so the unvouched files
are left out instead.

`verified-by: bravebot_agent::workspace::a_repo_map_holds_only_files_the_trust_map_vouches_for`
`verified-by: bravebot_agent::workspace::a_repo_map_never_opens_a_file_nobody_vouched_for`
`verified-by: bravebot_agent::turn::a_repo_map_shows_vouched_declarations_and_none_from_an_unvouched_file`

<a id="MAP-2"></a>
### MAP-2: a declaration is ranked by how many other vouched files mention its name

A name is credited once for each file that mentions it and does not declare it. A name declared in
several places shares that credit between them, so `new` and `main` do not outrank what a reader
came to find. Ties fall to path and then line, so the map is the same every time. The scanner is
hand written and line based: no parser, no dependency, work proportional to the text.

**Why.** The ranking is a decision made from file text, so it reads vouched text only, or a file
nobody vouched for could raise its own declaration to the top of the map by mentioning it.

`verified-by: bravebot_agent::workspace::a_mention_in_an_unvouched_file_does_not_raise_a_symbol`
`verified-by: bravebot_agent::repo_map::the_declaration_other_files_mention_comes_first`
`verified-by: bravebot_agent::repo_map::a_mention_in_the_declaring_file_earns_nothing`
`verified-by: bravebot_agent::repo_map::a_name_declared_in_many_places_shares_its_credit`
`verified-by: bravebot_agent::repo_map::the_same_sources_give_the_same_map`

<a id="MAP-3"></a>
### MAP-3: a file holding what looks like a credential contributes nothing

A declaration can be a binding with its value on the same line, so the file is scanned as any read
is ([CRED-15](../credential-protection.md#CRED-15)) and dropped whole on a finding. It is counted
together with the files that could not be read or were too large, in one number, so the result does
not say which file held a credential ([CRED-19](../credential-protection.md#CRED-19)).

`verified-by: bravebot_agent::workspace::a_repo_map_leaves_out_a_file_holding_a_credential`

<a id="MAP-4"></a>
### MAP-4: a file a deny rule covers, or that sits in a vendored or generated tree, is not mapped

Mapping a tree is reading it, so a rule that keeps a file from being read keeps it from the map, and
the file is not counted as left out, since a count would say it exists. The directories a listing
does not walk ([LIST-3](list-files.md#LIST-3)) are not walked here.

`verified-by: bravebot_agent::workspace::a_repo_map_does_not_open_a_file_a_deny_rule_covers`
`verified-by: bravebot_agent::workspace::a_repo_map_skips_noise_directories`

<a id="MAP-5"></a>
### MAP-5: a map is bounded by its budget and says what it cut

`budget` is in tokens, clamped to a range, and the declarations kept are the highest ranked that fit,
not the first in the file. A map that shows fewer declarations than it found, stopped at a cap on
files or bytes, or could not open a directory says so to the planner, and so does one that left out
files nobody vouched for. A directory named where a file is expected, and a file named where a
directory is, are refused.

**Why.** Silence reads as the whole tree, and a planner told nothing concludes a symbol does not
exist when the map was cut off.

`verified-by: bravebot_agent::repo_map::the_budget_bounds_the_body_and_the_count_says_what_it_cut`
`verified-by: bravebot_agent::repo_map::a_tight_budget_keeps_the_declaration_that_ranks_first_not_the_first_in_the_file`
`verified-by: bravebot_agent::repo_map::a_path_too_long_for_the_budget_does_not_empty_the_map`
`verified-by: bravebot_agent::workspace::a_repo_map_says_when_the_file_cap_stopped_it`
`verified-by: bravebot_agent::workspace::a_repo_map_of_a_file_is_refused`
