---
id: TRUST
title: The trust map
status: normative
governs:
  - crates/core/src/trust.rs
  - crates/core/src/file_authority.rs
  - crates/core/src/policy.rs
  - crates/tui/src/trust_prompt.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/dropped.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/scratch.rs
  - crates/cli/src/main.rs
  - crates/tui/src/app.rs
  - crates/ui-files/src/main.rs
guards:
  - symbol: TrustStore::trust
  - symbol: TrustStore::distrust
  - symbol: TrustStore::rebased
  - symbol: Policy::reconcile_after_write
  - symbol: Policy::vouch_for_named_path
documented-by: docs/website/docs/security/trust.md
---

## Scope

Which paths a user has vouched for, every way a rule enters that record, what a write does to
it, and how long an answer lasts. It does not cover what a label means once assigned, which is
[labels.md](labels.md), nor what may be released to whom, which is [routing.md](routing.md), nor
`~/.bravebot`, which the map does not govern (TRUST-11).

## The record

<a id="TRUST-1"></a>
### TRUST-1: nothing is trusted until it is granted

An empty map trusts no path. Trust is granted by a person and never inferred from silence, from
a path's shape, or from anything a model or a file said.

**Why.** This is what makes declining at startup mean something. A default of trusted would make
the answer decorative.

`verified-by: bravebot_core::trust::an_empty_store_trusts_nothing`

<a id="TRUST-2"></a>
### TRUST-2: the longest matching prefix decides

Rules are keyed by path prefix and matched by whole segments. Both polarities are expressible, so
a trusted tree may hold an untrusted subtree, which may hold a trusted path again. Equivalent
spellings of a path are one rule, and a later decision replaces an earlier one.

**Every key is a full path, and there is one namespace of them.** A caller may name a path
relatively, which is how a file in the project is named, and the map reads that under the working
directory it was made with: `src/main.rs` in a session working in `/work` is the rule
`/work/src/main.rs`, and the working directory's own rule is `/work`. So the project's own rule is
a path like any other rather than an empty prefix that covers everything, and a rule about a
directory somebody opened by name (TRUST-9) is in the same map without reaching into the project:
`/Users/x/notes` does not prefix `/Users/x/proj`, and whole-segment matching is what stops
`/Users/x/proj` covering `/Users/x/proj-secret`. A rule about a directory that *does* hold the
project covers the project's files, since the person vouched for a tree the project is in, and the
project's own rules are the more specific ones wherever they exist.

A rule is about a **path**, not about the files that were in it when the rule was made, and it is
consulted when a file is read rather than when the rule is written. A file that appears in a
trusted directory afterwards is therefore read as trusted, whoever put it there.

**Why.** Per-file exceptions in both directions are the only way `@vendor/lib.js` can be trusted
inside a `vendor` a person marked untrusted, without that answer leaking to its siblings.

**Why one namespace.** Two of them, one for names under the working directory and one for names
spelled in full, was the arrangement this replaced. It existed to stop the working directory's
empty prefix covering every absolute path there is, and a rule on `/` covering every relative one.
Full paths make both unreachable rather than guarded against, and a file that has a name of each
kind then has one rule instead of two that could disagree.

`verified-by: bravebot_core::trust::the_deepest_rule_wins_at_any_depth`
`verified-by: bravebot_core::trust::an_untrusted_subpath_overrides_a_trusted_parent`
`verified-by: bravebot_core::trust::a_trusted_subpath_overrides_an_untrusted_parent`
`verified-by: bravebot_core::trust::a_rule_matches_whole_segments_only`
`verified-by: bravebot_core::trust::equivalent_path_spellings_are_the_same_rule`
`verified-by: bravebot_core::trust::every_equivalent_spelling_of_a_path_reaches_the_same_rule`
`verified-by: bravebot_agent::workspace::a_second_spelling_of_a_distrusted_file_is_read_as_untrusted`
`verified-by: bravebot_core::trust::a_later_decision_replaces_an_earlier_one`
`verified-by: bravebot_core::trust::a_path_named_in_full_reaches_the_rule_its_relative_name_wrote`
`verified-by: bravebot_core::trust::a_rule_outside_the_working_directory_decides_nothing_inside_it`
`verified-by: bravebot_core::trust::trusting_the_workspace_says_nothing_about_an_added_directory`
`verified-by: bravebot_core::trust::a_directory_vouched_for_above_the_project_covers_the_project`
`verified-by: bravebot_core::trust::trusting_the_filesystem_root_covers_the_working_directory_too`
`verified-by: bravebot_core::trust::the_deepest_absolute_rule_wins`
`verified-by: bravebot_core::trust::one_added_directory_does_not_cover_a_sibling`
`verified-by: bravebot_core::trust::equivalent_absolute_spellings_are_the_same_rule`
`verified-by: bravebot_core::trust::every_equivalent_absolute_spelling_reaches_the_same_rule`

<a id="TRUST-3"></a>
### TRUST-3: withdrawn, relative and absolute rules are separate namespaces

Replaced by TRUST-2, which keys every rule under a full path, so that the collapse two namespaces
were kept apart to prevent is unreachable rather than guarded against, and by TRUST-18, which owns
which name a path is asked about.

## What a write does

<a id="TRUST-4"></a>
### TRUST-4: what a write asks, and what it records

Every row describes the effective trust after a complete write. A started write is recorded even
when its final trust matches the rule already covering the path. A refused operation or one that
has not entered its filesystem effect records no write.

| data | destination | prompt? | effect on the map |
|---|---|---|---|
| trusted | trusted | no | unchanged |
| untrusted | trusted | **yes** | that path becomes untrusted |
| trusted | untrusted | no | that path becomes trusted |
| untrusted | untrusted | no | unchanged |
| either | never mentioned | **yes** | that path takes the data's trust |

A prompt asks one question and only this one: **may this path stop being trusted?** That is the
only consequence a later step cannot undo, since a path recorded as untrusted can no longer be
examined or edited.

Parent and delegates use one live file authority. Before an effect may truncate or replace a
file, its exact path becomes untrusted. It remains untrusted while the effect is in progress,
and after any error following entry. Only a complete file replacement publishes its input's
trust. Another writer to an active path is refused before entering its own effect; other paths
remain available. A later complete trusted replacement can trust the path again.

Byte capture and label lookup share a boundary with effect entry and successful publication.
No reader can capture replacement bytes under an earlier grant. This covers context files,
attachments, deferred references and searches as well as direct reads. Collection never merges
a delegate's stale file snapshot. No coordination lock spans a prompt, model call or child join.

Approvals based on a preview are checked against the path's revision before effect entry or
vouching. A changed path cannot spend the old approval. Changes to another path do not invalidate
that approval. A backup records the trust of its bytes at capture under the same boundary.

`verified-by: bravebot_agent::turn::overlapping_delegate_writes_follow_effect_order_in_both_collection_orders`
`verified-by: bravebot_agent::workspace::reads_before_write_publication_and_failed_replacements_remain_untrusted`
`verified-by: bravebot_agent::workspace::shared_file_authority_preserves_aliases_scratch_added_paths_and_independent_writes`

**Why writing trusted data never asks.** Trusted data means the turn observed nothing untrusted,
so it holds no byte an attacker influenced, and the destination only ever gains trust. There is
nothing to ask about.

**Why untrusted data into a trusted path must mark it untrusted.** This closes the round trip.
Untrusted bytes are anything derived from the web or from a file outside a trusted path; written
into a trusted tree and read back as trusted they would launder injected text into trusted input,
and the map would become a bypass for the gate it exists to support.

**Why a path nobody has mentioned asks either way.** It differs from one deliberately marked
untrusted: the first has no decision behind it, so the first write there is the moment to ask.
This is also what makes declining at startup meaningful, since with nothing vouched for every
write is shown.

`verified-by: bravebot_core::policy::trusted_data_into_a_trusted_path_is_silent_and_changes_nothing`
`verified-by: bravebot_core::policy::untrusted_data_into_a_trusted_path_prompts_and_distrusts_the_path`
`verified-by: bravebot_core::policy::trusted_data_into_an_untrusted_path_is_silent_and_trusts_the_path`
`verified-by: bravebot_core::policy::untrusted_data_into_an_untrusted_path_is_silent_and_changes_nothing`
`verified-by: bravebot_core::policy::an_unvouched_path_prompts_either_way`
`verified-by: bravebot_core::policy::a_file_written_with_untrusted_data_reads_back_untrusted`
`verified-by: bravebot_core::policy::a_file_read_back_under_another_spelling_is_still_untrusted`

<a id="TRUST-5"></a>
### TRUST-5: reconciliation marks the exact path, never the parent

Reconciliation records the file written, and no directory above it.

**Why.** One untrusted file does not taint its siblings. Marking the parent would turn a single
fetched page into a project nobody may edit.

`verified-by: bravebot_core::policy::untrusted_data_into_a_trusted_path_prompts_and_distrusts_the_path`
`verified-by: bravebot_core::policy::trusted_data_into_an_untrusted_path_is_silent_and_trusts_the_path`

## How long an answer lasts

<a id="TRUST-6"></a>
### TRUST-6: the map belongs to the session, not the directory

Every session start asks, whatever any earlier session in that directory answered. `/clear` begins
a session and therefore asks. `--resume` does not ask, and restores the map from the record of the
session chosen; a record from before maps were kept has none, and is asked about.

**What the record keeps is the name, not the key.** A rule inside the project is written down
relative to it and a rule outside is written down in full, and a resume reads the relative ones
under the directory it is resuming into. The map holds full paths (TRUST-2), and a record of those
would fail quietly the moment the checkout was moved or renamed: every rule would name a path that
is no longer there, nothing would match, and the session would resume behaving as though nobody had
vouched for anything. Recording the name and putting the directory back on it at load is what keeps
a record about the same files.

**What is not the map is not covered by this.** An `allow` rule a checkout proposed and a person
granted is recorded per workspace, and a later session there honours it without asking
([permissions.md](permissions.md#PERM-15)). That is not trust inferred from what a file said, which
TRUST-1 rules out: it is an answer a person gave, about one rule text from one named file, and what a
rule decides is whether they are asked rather than what is trusted. The map is still asked about
afresh, so a session inheriting such a grant trusts no path nobody vouched for.

**Why.** The question grants standing permission. Honouring last week's answer grants it on behalf
of a user who was never asked, and trust assumed from silence is not trust granted. A resume is not
an exception: the answer honoured is the one that session's own user gave, and it carries the rules
that session's writes recorded, which is what stops a resumed turn reading back a file an earlier
turn of the same session poisoned.

**Why a granted rule may last where this answer may not.** This answer covers a whole tree and
everything that appears in it afterwards, so there is no narrowing that would make keeping it worth
the risk. A granted rule is the opposite shape: one text, from one file, in one workspace, lapsing
the moment the checkout edits it. What a later session honours unasked is therefore exactly what
somebody read, which is the property this answer cannot have.

`verified-by: bravebot_session::sessions::a_record_resumes_its_rules_under_the_directory_it_is_read_in`
`verified-by: bravebot_tui::app::a_fresh_session_is_asked_rather_than_inheriting_a_map`
`verified-by: bravebot_tui::app::a_resume_starts_with_the_map_its_own_record_kept`
`verified-by: bravebot_tui::app::a_record_from_before_maps_were_kept_is_asked_about`
`verified-by: bravebot_session::sessions::a_record_that_predates_the_map_has_none_rather_than_an_empty_one`
`verified-by: bravebot_session::sessions::a_distrusted_path_inside_a_trusted_tree_survives_the_record`
`verified-by: bravebot_session::sessions::two_recorded_spellings_of_one_path_resume_as_untrusted`
`verified-by: bravebot_tui::sessions::sessions_are_written_read_back_and_kept_per_directory`

<a id="TRUST-7"></a>
### TRUST-7: the startup question covers the whole workspace, and declining trusts nothing

At startup the user is asked whether they trust the working directory. Yes writes a rule covering
the tree. Declining writes nothing, so every write is shown. Leaving at the question starts no
session.

A session running in the mode that asks about nothing is the one exception: the question is not put,
and the map is the one a yes would have written. That mode approves vouching for every quarantined
file the planner reads, so the tree becomes trusted a file at a time whether or not the question is
asked, and a modal box is the most conspicuous thing there is to put to somebody who asked to be
asked about nothing. [permission-modes.md](permission-modes.md) is what selects that mode, and no
other mode may answer this question.

**Why the exception goes no further.** A resumed session takes the map from its own record even
there, since the question is not being put in that case either and the answer its user gave is the
more specific record.

`verified-by: bravebot_tui::trust_prompt::trusting_covers_the_whole_workspace`
`verified-by: bravebot_tui::trust_prompt::only_y_trusts_and_enter_answers_nothing`
`verified-by: bravebot_tui::trust_prompt::declining_trusts_nothing`
`verified-by: bravebot_tui::trust_prompt::leaving_starts_no_session`
`verified-by: bravebot_tui::trust_prompt::ctrl_c_leaves_rather_than_answering_the_question`
`verified-by: bravebot_tui::trust_prompt::bypassing_trusts_the_workspace_instead_of_asking`
`verified-by: bravebot_tui::trust_prompt::every_other_mode_leaves_the_question_to_the_person`
`verified-by: bravebot_tui::app::a_resume_keeps_its_own_map_even_where_the_mode_would_answer`

## The ways a rule is written

Each grants exactly one thing and grants it because a person made a gesture, never because
anything inspected content. TRUST-7 is the first. Three more write a rule the same way and have
specs of their own: naming a file is [naming-files.md](naming-files.md), dropping one on the
window is [dropping.md](dropping.md), and accepting a directory a settings file asked for is
[permissions.md](permissions.md). A file naming that directory writes nothing by itself.

<a id="TRUST-8"></a>
### TRUST-8: a quarantined read offers the same rule, at the moment it bites

When a turn reads a file nobody has vouched for, the user is shown the path and the first lines of
it and asked whether to trust it. Yes writes exactly the rule `@` would have written. Asked once
per path per turn, and only where the read is quarantined. Declining leaves the file as it was and
the turn carries on with a reference.

What the file holds decides nothing about whether the question is put. A file with nothing to show,
because it is empty or does not read as text, is asked about like any other, and the prompt says so
where the preview would be.

What does decide it is the path, and the question is put only where the path names a file. Never a
picture, because what a yes grants is that a file's text may be read and a picture's text is never
read whatever the map says. Never a directory or a path that names nothing either: a yes writes a
rule covering everything beneath the name it was given, so a prompt titled with one file would hand
over the trust half of what [`/add-dir`](#TRUST-9) grants, every file beneath the name at once, over
a string the planner chose rather than one a person typed. Reach it would not grant, which is the
only part of that gesture this could not have imitated.

A confined check reads the file before the question is drawn, and the word it gave is on the screen
with the preview. It reads **the whole file** and not the preview, because a yes grants the whole
file. That is [CHECK-10](vetting.md#CHECK-10), and it decides nothing here: a yes writes the rule
whatever the word was, and a no writes nothing whatever the word was.

```
╭ let the model read this file? ────────────────────────────╮
│Trust game.js                                              │
│                                                           │
│  the check found no attempt to give instructions in this  │
│                                                           │
│  the model cannot read this file, so it is working blind  │
│  on it. Vouching lets it read this file for the rest of   │
│  this session, here and in every later read.              │
│                                                           │
│┃ const SPEED = 100;                                       │
│                                                           │
│  y trust it    n leave it quarantined    ctrl-c stop      │
╰───────────────────────────────────────────────────────────╯
```

**Why.** This is the map's own decision offered where it matters, not a second route to trusting
content, so a yes stays consistent for every later read. It exists because of a session that did
not have it: asked to fix a bug in a game it could not read, the model pointed an isolated
processor at the file, wrote the answer back unseen, and finished by saying it could not confirm
any of what it had done. One prompt would have let it read the file.

**Why a check on it.** A session read a file holding an injection attempt, was offered this
question with nothing on the screen but the first lines, and the person had to answer it blind.
They said no, the planner reached for the same file through `run` instead, and *that* path ran a
check and caught the attempt. The prompt that grants the most was the one saying the least.

`verified-by: bravebot_agent::turn::a_quarantined_read_offers_the_user_the_chance_to_vouch`
`verified-by: bravebot_agent::turn::a_vouch_offer_carries_what_a_check_said_about_the_whole_file`
`verified-by: bravebot_tui::confirm::the_vouch_prompt_says_what_a_check_found`
`verified-by: bravebot_agent::turn::a_quarantined_file_with_nothing_in_it_is_still_offered_for_vouching`
`verified-by: bravebot_agent::turn::a_picture_is_not_offered_for_vouching`
`verified-by: bravebot_agent::turn::a_path_that_names_no_file_is_not_offered_for_vouching`
`verified-by: bravebot_agent::turn::declining_to_vouch_leaves_the_file_quarantined`
`verified-by: bravebot_agent::turn::a_trusted_file_is_not_offered_for_vouching`
`verified-by: bravebot_agent::turn::the_same_file_is_offered_once_per_turn`
`verified-by: bravebot_tui::confirm::a_preview_with_nothing_in_it_says_so`

<a id="TRUST-9"></a>
### TRUST-9: `/add-dir` makes a directory both reachable and trusted, for the session

`/add-dir ~/notes` records a rule under that directory's own path (TRUST-2) that does two things
together: the directory becomes reachable, since an absolute path is otherwise refused whatever the
map says, and it is recorded as trusted. It lasts the session, `--resume` carries both halves, and `/clear` closes it.
A directory already inside the project is refused. A directory a resume cannot open again, because
it has moved or gone, is said so rather than passed over.

A directory that *holds* the project is not refused, and the rule it records covers the project's
files as it covers everything else in that tree. Vouching for a directory is a standing statement
about the place (TRUST-2) and the project is in it, so there is nothing to except. The project's own
rules are the more specific ones and still decide wherever they exist, which is what keeps a no
given inside the project from being undone by a yes given above it.

**Why.** Either half alone is no use, one leaving a rule about files nothing can open and the other
leaving a directory that prompts on every edit. It closes with the session for the reason every
other answer here does (TRUST-6): leaving a tree reachable once nothing vouches for it would
outlive the answer that allowed it.

`verified-by: bravebot_agent::workspace::a_file_in_an_added_directory_is_readable_by_its_absolute_path`
`verified-by: bravebot_agent::workspace::closing_added_directories_makes_them_unreachable_again`
`verified-by: bravebot_agent::workspace::a_new_file_can_be_created_in_an_added_directory`
`verified-by: bravebot_agent::turn::a_turn_can_read_a_file_in_an_added_directory`
`verified-by: bravebot_tui::sessions::a_resumed_session_can_still_open_the_directory_it_added`
`verified-by: bravebot_tui::sessions::a_directory_that_has_gone_since_is_reported_on_resume`

## Boundaries

<a id="TRUST-10"></a>
### TRUST-10: no rule extends reach; reading, writing and listing stay confined

Reading, writing, editing, listing and searching are confined to the working directory and to
whatever has been opened beside it, the directory the session was given among them (TRUST-16). `..`
and an absolute path outside those are refused rather than resolved, in an added directory exactly as
in the project, and a symlink leaving one is refused.
A relative path always means the project, so no file has two spellings. Naming a directory
includes nothing, since a directory is somewhere to type through rather than a file to read.

Confinement is decided by where an operation lands and not by how its path is spelled, so it holds
for a file that does not exist yet: a write creates what it names, and a symlink out of the tree is
refused whether or not there is anything at the other end of it.

**Why.** A directory symlink is an ordinary entry in a repository, so confinement that tested only
existing paths would let a checkout choose where a write lands. A person approved a path and the
prompt showed that path, so the bytes go there.

`verified-by: bravebot_agent::workspace::an_absolute_path_outside_every_added_directory_is_still_refused`
`verified-by: bravebot_agent::workspace::a_parent_component_cannot_climb_out_of_an_added_directory`
`verified-by: bravebot_agent::workspace::a_symlink_out_of_an_added_directory_is_refused`
`verified-by: bravebot_agent::workspace::creating_a_file_through_a_symlinked_directory_out_of_the_workspace_is_refused`
`verified-by: bravebot_agent::workspace::creating_a_file_through_a_symlinked_directory_in_an_added_directory_is_refused`
`verified-by: bravebot_agent::workspace::writing_to_a_dangling_symlink_out_of_the_workspace_is_refused`
`verified-by: bravebot_agent::workspace::overwriting_a_file_through_a_symlink_out_of_the_workspace_is_refused`
`verified-by: bravebot_agent::workspace::creating_a_file_through_a_symlink_inside_the_workspace_returns_where_it_landed`
`verified-by: bravebot_agent::workspace::writing_to_a_dangling_symlink_inside_the_workspace_lands_at_its_target`
`verified-by: bravebot_agent::workspace::a_destination_reached_through_a_dangling_symlink_out_of_the_workspace_is_refused`

<a id="TRUST-11"></a>
### TRUST-11: the map does not govern `~/.bravebot`

The user's own directory is read as trusted by provenance rather than by any rule here. A
project's own files are **not** covered by that and are read through this spec, whatever their
names. What is kept in that directory and how it is found is
[instructions.md](instructions.md); what it is trusted for is [skills.md](skills.md).

**Why.** The map is keyed by workspace-relative paths and has nothing to say about a path outside
the workspace. Asking it about one would be laundering.

`verified-by: bravebot_agent::skills::a_skill_the_trust_map_distrusts_stops_being_offered`

<a id="TRUST-12"></a>
### TRUST-12: `/status` lists the rules in force

Every rule the session holds is readable back, so what a line vouched for does not have to be
remembered.

`verified-by: bravebot_tui::status::an_added_directory_is_reported`
`verified-by: bravebot_tui::status::every_trust_rule_is_listed_however_many_there_are`

## Moving the working directory

<a id="TRUST-13"></a>
### TRUST-13: `/cd` moves the working directory, and the map moves with it

`/cd ~/projects/other` makes that directory the working directory: it is what a relative path
means from then on, where commands run, and where the project's own instructions are looked for.
The directory left behind closes, and so does any directory opened by name that holds the new one
or sits inside it, each said out loud as it happens.

The map travels with the session and says what it always said. Every rule is keyed on the full path
it is about (TRUST-2), so a working directory that moves carries none of them with it: each still
names the file it always named, and what changes is which of them a relative name reaches and how
each is spelled back. The new directory is then vouched for, on the same footing as `/add-dir`:
the person typed the path, and a later decision replaces an earlier one.

**Why.** A yes given for one project must not become a yes for another, and a no given inside the
old one must not be forgotten. Both follow from a rule naming the file rather than the directory it
was written from, and neither grants nor withdraws anything, which is what makes this something
that can be done without asking.

Nothing may overlap the new working directory, and that is not tidiness: a directory that holds the
new root, or sits inside it, is reachable through the root already, so leaving it open would record
a second open directory for a path to be named under (TRUST-18) with nothing to choose between
them.

`verified-by: bravebot_core::trust::a_yes_for_one_project_does_not_follow_a_move_to_another`
`verified-by: bravebot_core::trust::a_no_inside_the_new_directory_survives_the_move`
`verified-by: bravebot_core::trust::a_rule_in_an_added_directory_becomes_relative_when_it_is_moved_into`
`verified-by: bravebot_core::trust::rebasing_neither_grants_nor_withdraws_anything`
`verified-by: bravebot_agent::workspace::a_relative_path_means_the_new_working_directory_once_it_has_moved`
`verified-by: bravebot_agent::workspace::moving_closes_the_directory_left_behind`
`verified-by: bravebot_agent::workspace::moving_closes_an_added_directory_that_overlaps_the_new_one`
`verified-by: bravebot_agent::workspace::moving_leaves_an_unrelated_added_directory_open`
`verified-by: bravebot_tui::app::changing_directory_moves_the_workspace_and_vouches_for_where_it_moved`
`verified-by: bravebot_tui::app::changing_directory_leaves_the_previous_answer_where_it_was_given`
`verified-by: bravebot_tui::app::moving_into_a_directory_keeps_the_answers_given_inside_it`

## A scratch directory outside the workspace

Without a directory of its own a session has nowhere to put an intermediate file. TRUST-10 confines
reading, writing, editing, listing and searching to the working directory and to whatever was opened
beside it, and decides confinement by where an operation lands rather than by how a path is spelled. A redirection target on a command
line is a write destination held to that same confinement
([tools/command-line.md](tools/command-line.md)), so `cargo metadata > /tmp/meta.json` is refused
before the map is consulted at all. That is not a missing grant. The map decides which reachable
paths prompt, an unreachable path never reaches that question, and so no answer to that question,
however wide, makes `/tmp` writable. Only making the path reachable would, and the one route a
person has to that is `/add-dir /tmp`, which under TRUST-9 marks whatever every other process on the
machine has left in a world-writable directory as trusted input in the same action. What is left is
writing into the project, where the file is untracked and a build, a test run, a `git add -A` and a
reviewer each have to deal with it, and somebody has to remember to delete it.

<a id="TRUST-14"></a>
### TRUST-14: a session is given a directory of its own, outside the project

A session has a directory of its own in the system temporary directory, created as the session
opens. Its name carries this program's prefix and enough besides to tell two sessions apart. It is
created rather than opened, so a name something else holds is refused rather than adopted, and on
Unix it is created with mode `0700`. `/status` reports it as the session's own. A one-shot run has
one for as long as it runs. A session that cannot be given one runs without one and says so.

**Why.** Both of the other places to put an intermediate file cost more than this one. In the
project it is a file a build, a test run, a `git add -A` and a reviewer each have to deal with, and
one somebody has to remember to delete; it is also a file several walks would each have to be taught
to skip, none of them reading a project's ignore file, on the ground that a tree that can hide its
files from a search can hide them from review ([tools/search.md](tools/search.md)). Reached by
`/add-dir /tmp` it costs the trust that line grants in the same action, over everything every other
process on the machine has left in a world-writable directory. Out here neither is true.

Which directory the platform keeps temporary files in is a question the standard library already
answers: `TMPDIR` on Unix, falling back to `/tmp`, and on Windows the operating system's own answer,
which resolves `TMP`, then `TEMP`, then the profile directory. An undefined `TMPDIR` therefore needs
nothing written here.

Created rather than opened, and `0700`, because that directory is shared. On macOS it is the
account's own, but on Linux it is ordinarily the world-writable `/tmp`, where a name this program
composes is one another account can create first, or leave pointing at a file of theirs. Refusing a
name already taken is what keeps a write from going through one of those, and the mode is what keeps
another account away from what lands inside, because a file arriving there comes in at a umask
whether this program opened it for a redirection or a program wrote it itself. It is what the editor
hand-off already does with the one file it has to write ([incognito.md](incognito.md#INCOG-8)). The
mode is Unix's alone: the Windows temporary path is ordinarily the account's own and takes its
permissions from that, and the refusal to reuse a name covers the rest.

Enough in the name to tell two apart rather than a fixed name, because two sessions sharing one
directory could each read and overwrite what the other wrote, and because a fixed name is one
another account can take first. Not the session's identifier either, which would name a session in
a directory anybody on the machine can list.

Saying so rather than refusing to open the session, because a session with nowhere to put an
intermediate file can still do everything else.

`verified-by: bravebot_agent::scratch::a_session_is_given_a_directory_of_its_own`
`verified-by: bravebot_agent::scratch::a_name_something_else_holds_is_refused`
`verified-by: bravebot_agent::scratch::nobody_else_may_read_what_a_session_writes_there`
`verified-by: bravebot_tui::status::the_sessions_scratch_directory_is_reported_for_what_it_is`
`verified-by: bravebot_tui::status::a_session_with_no_scratch_directory_reports_none`

<a id="TRUST-15"></a>
### TRUST-15: nothing in the session's directory outlives the session

The directory goes as the session ends, with everything written in it, whether the session ended by
being left or by an error on the way out. A session that carries on from another is given its own: a
resume, a fork, and the session `/clear` begins each open a new directory, and the one belonging to
the session they followed is removed rather than handed on.

**Why.** A resume can come days later, and bytes surviving that gap are a cache nothing evicts.
There is nothing to reconstruct anyway, since the name carries what tells one directory from its
neighbours rather than the session's identifier. What a cleared context wrote is not something the
session after it should find lying there either.

A `/cd` needs nothing done to the directory, which is the one lifetime rule this location removes
rather than restates: the directory is not under the working directory, so a move does not leave it
behind, and there is no ordering to get right between removing it and still being able to reach it
(TRUST-13).

`verified-by: bravebot_agent::scratch::nothing_written_in_it_outlives_the_session`
`verified-by: bravebot_agent::scratch::no_two_sessions_are_given_the_same_directory`

<a id="TRUST-16"></a>
### TRUST-16: the session's directory is reachable, and its reach grants no trust

A path under the session's own directory resolves, so a file there is read, written, listed and
searched by its absolute path. What that reach covers is that directory: the temporary directory it
sits in is not reachable, a path leaving it is refused as one leaving the project is (TRUST-10), and
it is not one of the directories the user opened by name.

Neither `/add-dir` nor `/cd` takes it, and a directory inside it is refused the same way. Adding it
would write a rule over a directory whose whole point is carrying none, and it is reachable without
one already. Moving into it would make the working directory a directory this session removes when it
ends, leaving every read after that against a directory that is no longer there. The temporary
directory it sits in is not refused, since that is a directory a person may open and their answer
about it is theirs to give.

Nothing is trusted for being there. The directory carries no rule of its own, and a path under it
that no rule covers is answered by what was said about the workspace (TRUST-7), which trusts nothing
where it was declined. Per file the answer is reconciliation's, marking the exact path a write went
to (TRUST-4, TRUST-5), and a rule recorded inside the directory answers for the file it names and
weakens what is read along with it.

**Why.** The directory is outside the working directory, so nothing reaches it unless something says
so, and what says so here is this program rather than a person. The grounds are that the directory
was created here, empty, owned by this account and readable by nobody else, so there is no content in
it anybody could be asked to vouch for. Those are grounds about reach and they do not extend to
trust: TRUST-1 is that nothing is trusted until it is granted, and a directory this program created
is not a person's answer. Emptiness is also true only for an instant, and what keeps it true
afterwards is the ownership and the mode rather than anything the map records.

The workspace's answer rather than no answer, because a path nobody has said anything about prompts
on every write and is quarantined on every read (TRUST-4). In a directory whose whole purpose is
being written to and read back that is a prompt for every intermediate file and a quarantine on
reading one back, which is a session asked about its own workings instead of about the work. What was
said about the workspace is what this session already decided about the files a turn deals with, so
taking it here asks what the workspace asks and no more.

The rules recorded inside the directory still weaken the answer, because a line whose output is
untrusted distrusts the file it redirected into, under the name the line spelled
([tools/command-line.md](tools/command-line.md)). Without that a turn could read its own untrusted
output back as trusted through the directory around it, which is the case worth having right in a
place intermediate files are written.

This is deliberately not what `/add-dir` does. TRUST-9 grants reach and trust together and says
either half alone is no use, one leaving a rule about files nothing can open and the other a
directory that prompts on every edit. Neither is what happens here, because the reach comes from this
program while the label comes from the workspace's own answer, so the directory prompts exactly when
the workspace prompts and no more. What `/add-dir` would add on top of that is trust the startup
answer withheld, over a directory nobody looked at.

A standing quarantine is refused for what it costs at the other end. A line a person vouched for
prints trusted output, and a redirection on such a line leaves the map as it was, so a standing
quarantine would make a plan pay a prompt to read back the file it just asked for. A default every
session works around is not a default.

It is a place to write and not a place writes stop being asked about. A path under it appears in a
plan's write set, is shown in the prompt, and takes every write gate a path in the project takes
([tools/command-line.md](tools/command-line.md)). What it buys is a fixed place that needs no name
invented for it, that a repository does not report, that no walk has to skip, and that goes when the
session does. It buys no prompt anybody would otherwise see.

One directory inside the temporary directory is reached and not the directory that one sits in, so
`/tmp` is no more writable than it was and the reason `/add-dir /tmp` is the wrong answer is
untouched. A directory reachable without a rule is also why `/status` has a line of its own for it
(TRUST-14): the rules in force (TRUST-12) cannot report a directory that has none, and reach nothing
says out loud is reach somebody discovers by accident.

`verified-by: bravebot_agent::workspace::a_file_in_the_sessions_own_directory_is_reachable_by_its_absolute_path`
`verified-by: bravebot_agent::workspace::the_temporary_directory_around_it_stays_unreachable`
`verified-by: bravebot_agent::workspace::a_symlink_out_of_the_sessions_own_directory_is_refused`
`verified-by: bravebot_agent::workspace::the_sessions_own_directory_is_not_one_the_user_added`
`verified-by: bravebot_agent::workspace::the_sessions_own_directory_cannot_be_added_by_name`
`verified-by: bravebot_agent::workspace::the_working_directory_cannot_be_moved_into_the_sessions_own_directory`
`verified-by: bravebot_agent::workspace::a_second_spelling_of_a_file_in_the_sessions_own_directory_reaches_the_same_rule`
`verified-by: bravebot_core::policy::a_write_in_the_sessions_own_directory_asks_what_a_write_in_the_project_asks`
`verified-by: bravebot_core::policy::a_declined_workspace_leaves_the_sessions_own_directory_untrusted`
`verified-by: bravebot_core::policy::an_untrusted_file_in_the_sessions_own_directory_stays_untrusted`
`verified-by: bravebot_core::policy::a_name_climbing_out_of_the_sessions_own_directory_is_not_answered_for_it`
`verified-by: bravebot_core::policy::a_line_reading_the_sessions_own_directory_still_sees_the_rules_inside_it`

<a id="TRUST-17"></a>
### TRUST-17: a program a `run` starts is told where the session's directory is

Every stage of a line is spawned with `BRAVEBOT_SCRATCH_DIR` holding the path of the directory this
session was given (TRUST-14), in the background as in the foreground. An assignment of that name on
the line wins, as the same assignment in front of a program wins in a shell
([tools/command-line.md](tools/command-line.md)). A session that has no directory of its own sets
nothing, so a program finds the name absent rather than pointing at a directory that is not there.

**Given as the session opens, not asked for by a tool.** The directory is there before the first
line runs and its path is in the environment each stage is spawned with, so a program that wants
somewhere to write finds it without anything having been called first. The planner is told the
same way and at the same point, as one of the facts the system prompt states about this machine
([instructions.md](instructions.md)), since a variable nothing knows to read is a variable nothing
reads.

A tool that makes one on demand is the alternative, and is refused. It would cost a description in
every request whether or not a turn needs a scratch file at all, and the first line that redirects
would be written before the tool had been called, so a turn pays a refusal and a round trip to learn
what an environment variable would have told it for nothing.

The line itself expands nothing. A `$` in a line is refused, and quoted it is ordinary text
([tools/command-line.md](tools/command-line.md)), so what reads the variable is the program the line
named rather than anything between the two. That is why the name is worth having: an argument a
person endorsed is the text they were shown, and a program that needs the path reads it where paths
of that kind are kept.

Set on each stage as it is spawned rather than written into the environment this process holds. This
process reads its own environment from several threads, so rewriting that in place has no moment at
which nothing is reading it, and reaching the programs a line starts is the whole of what the
variable is for. A line the person typed themselves has nothing added to it, keeping their own
environment as it keeps everything else about their terminal ([RUN-12](tools/run.md#RUN-12)).

`$TMPDIR` for a program a `run` starts stays what it was, since the map governs the operations this
program performs and the redirection targets a line spells rather than what a program opens for
itself. A program's own temporary file is named in no plan and read back by nothing, so pointing that
variable at the session's directory would put files nobody decided anything about inside a directory
the map answers for, which is the known cost about a file another process drops in, arrived at on
purpose. Whether a confined program reaches a
temporary directory at all is a question for the base profile [sandboxing.md](sandboxing.md)
describes.

`verified-by: bravebot_agent::exec::a_stage_is_told_where_the_sessions_own_directory_is`
`verified-by: bravebot_agent::exec::a_background_stage_is_told_where_the_sessions_own_directory_is`
`verified-by: bravebot_agent::exec::a_steps_own_assignment_wins_over_the_directory_it_was_given`
`verified-by: bravebot_agent::exec::a_session_with_no_directory_of_its_own_names_none`
`verified-by: bravebot_agent::exec::a_stage_keeps_the_temporary_directory_this_process_has`
`verified-by: bravebot_agent::turn::a_program_a_turn_runs_is_told_where_the_sessions_directory_is`

## How a path is named to the map

<a id="TRUST-18"></a>
### TRUST-18: a path is asked about under the name of the open directory it lands in

**A key is `/`-spelled, and a name that is not one is read under the working directory.** A drive
letter is not a root here and a backslash is not a separator: a key arrives spelled from `/`, and a
backslash is a legal filename byte where paths are, so a file called `C:\notes` is a file in the
project and its name has to reach the project's rules. Resolving a platform's path into a key
therefore belongs to the workspace and not to the map, and every door that opens a directory by
name, `/cd` as much as `/add-dir`, refuses a resolved name it cannot spell that way rather than
handing over one that would be read as a path inside the project.

Every rule is written about a directory somebody opened, under the name that directory was opened
as: the working directory itself, which is what the startup answer covers (TRUST-7), and the path
it resolved to for one opened by name (TRUST-9). A path is therefore reduced to the open directory
it lands in, taking that directory's recorded name with the rest of the path as it was spelled.
This is the rule that two spellings of a path are one rule (TRUST-2), extended to the spellings
only a filesystem can tell apart: a directory named through a link is the same directory, and
`/tmp` being a link to `/private/tmp` is what a person types rather than a corner. A path landing
in no open directory is asked about as written, and so is one that reaches an open directory by a
link straight into the middle of it rather than through that directory's own name: neither has a
spelling under a recorded name, and nothing covers either.

A `..` component keeps the name it was given. Confinement refuses such a path rather than resolving
it (TRUST-10), so it is refused before anything reads it, and reducing it here would be guessing at
which file it named.

Where the path lands decides which directory's name it takes and the spelling decides the rest, and
each half answers a different question. A name spelled inside the project that lands out of it
takes the name of the directory it lands in and not the project's, because the project has no rules
about a file it does not hold and confinement refuses that name's relative spelling outright.
Taking only the ancestor that reaches an open directory, rather than the whole destination, is what
keeps the reduction from being a laundering step of its own: resolving the rest would return the
rule for a *different* name, so a file in an untrusted subtree would be readable as trusted through
a link inside that subtree. A file with two names of its own therefore still has two rules, which
is a cost of keying on the name and is written down below.

The reduction is the workspace's, so it holds for what the workspace does: the reads, writes,
listings, vouches and quarantines that go through it. A proven `run` line has no reduction to make,
because it need not settle which name is the file's: it asks this map under every name the operand
has and labels its output from the weakest answer
([tools/command-line.md](tools/command-line.md#CMDLINE-8)). That is stronger than the reduction
wherever the two differ, and it is the form that road can hold, since nothing there has a filesystem
to settle a name with. One thing stays outside both: the prompt before a write asks under the
spelling it was given, since it also matches the rules a person wrote in advance and those are
matched on the path as written, so it asks where a reduced name would not have.

**Why.** Without the reduction a directory reached by a second spelling of its own name is covered
by nothing, so a file the user vouched for is quarantined under half its names; on macOS that is
the ordinary case rather than a corner, since `/tmp` and `$TMPDIR` are both links. And the refusal
is the fail-closed half of the first paragraph: a rule keyed under a name with no leading slash
would be read as a path inside the project, where the answer given about the project at startup
covers it.

`verified-by: bravebot_core::trust::a_name_that_is_a_root_on_another_platform_is_read_under_the_working_directory`
`verified-by: bravebot_agent::workspace::a_directory_the_trust_map_cannot_key_is_refused`
`verified-by: bravebot_agent::workspace::a_project_file_named_absolutely_is_read_under_its_relative_rule`
`verified-by: bravebot_core::policy::a_project_file_named_absolutely_is_answered_by_the_project_rule`
`verified-by: bravebot_agent::workspace::a_file_reached_through_a_link_out_of_the_project_keeps_its_own_rule`
`verified-by: bravebot_agent::workspace::a_file_in_an_added_directory_named_through_a_symlinked_ancestor_keeps_its_rule`
`verified-by: bravebot_agent::workspace::a_project_file_named_through_a_symlinked_ancestor_is_read_under_its_relative_rule`
`verified-by: bravebot_agent::workspace::a_file_reached_by_a_link_into_the_middle_of_an_added_directory_is_not_covered_by_its_rule`
`verified-by: bravebot_agent::turn::vouching_for_a_project_file_named_absolutely_records_its_relative_rule`

## What a rewind puts back

<a id="TRUST-19"></a>
### TRUST-19: what a turn writes in the session's own directory is not rewound

A write through a file tool into the session's own directory keeps nothing to put back. Undoing the
turn neither restores such a file nor removes one the turn created, does not name it among the paths
it could not put back, and leaves the bytes it would have kept to the files the turn wrote in the
project.

**Why.** What one turn keeps so that it can be undone is bounded, and the bound is shared by every
file that turn wrote ([sessions.md](sessions.md)). An intermediate file is the size of thing that
bound is set to stay clear of, so keeping one spends what a source file's own copy needed and leaves
that file unable to go back: the file somebody wants undone loses to the file nobody does.

There is nothing there anybody has open either. The directory holds what a turn wrote for its own
use, it is empty when the session begins, and it goes when the session ends, so a file in it is
nobody's work in progress. What the undone turn wrote there stays until the session ends, and a
later turn in the same session can read it.

Removing what the turn created there, which costs none of the budget, is the alternative and is
refused for what it makes the report say. What a turn overwrote there cannot go back, since those
bytes are the ones this declines to keep, so a path in the directory would have to be named on the
line reporting the rewind, and that line would carry intermediate files for as long as the list of
them ran. A stale intermediate file in a directory whose whole purpose is intermediate files is the
smaller cost of the two.

This is what being outside the project buys rather than a carve-out inside it. The rule is that what
an undo keeps is what is in the project, and a directory the user opened by name is in the project
for this purpose: those are files somebody has open, which is the whole reason the bytes are kept.

`verified-by: bravebot_agent::workspace::a_write_in_the_sessions_own_directory_is_not_kept_for_an_undo`
`verified-by: bravebot_agent::workspace::a_write_in_the_sessions_own_directory_leaves_the_budget_for_the_project`

## What the session's directory is still missing

An incognito session has one as well, and [incognito.md](incognito.md) names it among the things
that still reach the filesystem in that mode: the name says nothing about which project or which
session, and nothing in it outlives the session, so what an empty one records is that this program
ran at this time rather than that a session ran in this project at this time.

**What has to exist first.**

- A leftover cannot be told from a live one without a claim. Removal as a session closes covers a
  session that closes, but a killed process leaves its directory, and the names are unrelated to
  each other precisely so that concurrent sessions cannot collide, so a later session cannot tell a
  leftover from the directory of a session running beside it. Out here it can be given the claim the
  in-project version could not have, because every session's directory sits in one place any session
  can read: a lock file each session holds open for its life makes the sweep a matter of trying the
  lock on each directory found and removing the ones nothing holds, after checking each is this
  account's own at the mode it should have. `flock` is reached through `rustix`, which is already a
  dependency; the Windows equivalent is `LockFileEx` and nothing here offers it today, so the sweep
  begins as Unix's and Windows leans on its own cleaner until it does.

## A file helper a surface hands a directory to

A surface that draws files of its own reads and writes them through a helper it hands a directory
to, rather than through the turn's own file tools: what it shows a person is not something a turn
read and nothing it writes there enters a turn. The confinement that helper performs is
[layering.md](layering.md)'s, which is that every path component is resolved relative to the handed
directory and no link is followed, so nothing can leave the tree. What is left is which directory it
may be handed, which is this document's.

<a id="TRUST-20"></a>
### TRUST-20: a directory such a helper is pinned to is one a person opened, and the helper grants nothing

The directory is an absolute path a person chose: the working directory a session was given, or the
state directory. Never a path a model wrote, and never one that arrived from the surface drawing the
files, which has only the directories it was already told about to name. A relative one is refused
rather than resolved against anything, since there is no directory it could be relative to that a
person named.

Being handed such a directory grants nothing. It is not a rule in the map, so it neither trusts what
is under it nor makes any of it reachable by a turn: bytes a person read or wrote this way have not
been vouched for, and a turn asking for the same path is answered by whatever the map says about it.
The map is where the question of what a *turn* may read is settled, and this road does not reach it.

**Why a person's directory and not any absolute path.** Refusing a relative path bounds where the
walk starts and says nothing about which tree it starts in, and a helper that resolved each component
faithfully under a directory the caller chose freely would still read any file on the machine. The
grounds for reading these bytes are that somebody opened this place, so the place has to be one they
opened.

**Why the grant is nil.** The rules here are about what a turn may read, and the reason they are
worth having is that a turn's context is what an attacker is trying to reach. Bytes on a person's
screen are not in that context. Treating a read here as vouching would write standing rules about a
whole tree out of somebody looking at a file, which is the largest grant this document has
([TRUST-9](#TRUST-9)) issued for the smallest gesture. Writing there is the same: what a person
edited themselves is theirs, and a turn reading it later is a read like any other.

**The state directory is admitted for what it is.** [TRUST-11](#TRUST-11) is that the map does not
govern `~/.bravebot`, so a path under it has no rule to be answered by, and its standing comes from
whose directory it is rather than from anything here. That is the same footing the rest of this
system reads it on, and it is why pinning to it is not an exception to the paragraph above but an
instance of it: it is the person's own directory.

`verified-by: bravebot_ui_files::main::a_root_that_is_not_an_absolute_path_is_refused`
`verified-by: bravebot_ui_files::main::a_symlink_replacing_the_root_or_leaf_is_refused`
`verified-by: bravebot_ui_files::main::traversal_nonregular_binary_and_oversized_reads_are_bounded`
`verified-by: by-construction (the helper is a process of its own that runs no command and speaks to no turn, so a byte it read reaches a screen and has no road into a turn's context: it holds no labelled value, mints no witness, and writes nothing the map is consulted about)`

## Known costs

Accepted deliberately. Do not "fix" one without changing this spec first.

- **A fresh session forgets what an earlier one poisoned.** The rule that untrusted data marks
  its destination untrusted holds within a session and
  across a resume of it. Across a fresh start it cannot, because the map it was recorded in is
  gone, so a file one session marked untrusted is read as trusted by the next session that vouches
  for the directory. The alternative is a per-directory map, which is a directory that trusts
  itself. If a file holds content you do not trust, the answer is to say no to the directory, or
  to not leave it there.
- **A file another process drops into a trusted directory is trusted.** TRUST-2 makes the rule
  about the path, so `npm install`, `git pull`, an editor, a background daemon, or a program the
  agent was allowed to run can all put a file inside a vouched-for tree and it will be read as
  trusted. TRUST-5 only fires on writes this system performs, so it never sees these.

  A redirection is the one exception: the harness opens that file itself, so what a line writes
  through `>` or `>>` is recorded
  ([tools/command-line.md](tools/command-line.md#CMDLINE-5)), under the name the line spelled and
  with the keying cost every other write has (below). A file the program opens on its own, which
  is `cmd -o notes.txt` or anything a build writes, is not.

  This is not an oversight and cannot be closed by watching the filesystem: by the time anything
  noticed, the question would be whether to distrust a file the user may have created themselves,
  and asking that on every change would make the map useless. What vouching for a directory means
  is a standing statement about that place, not about a set of files.

  The practical consequence is worth saying plainly: trusting a directory trusts what lands in it,
  so a tree that a build or a dependency manager writes into is a tree you are vouching for
  ahead of time.

  A scratch directory raises how often this runs without changing what it is. Writing a file and
  reading it back is incidental in a project and is the whole purpose of the directory above, and
  `cmd -o` into it is the ordinary case there rather than the odd one, so the exception in the
  second paragraph covers less of the traffic than it does elsewhere. No rule about that would
  help, since the same writes to the same effect are available one directory up: it is the standing
  statement about the place, met more often.
- **An undone turn's intermediate files stay until the session ends.** Nothing written in the
  session's own directory goes back (TRUST-19), so a turn that wrote one there and was undone has
  left a file a later turn in the same session can read. The directory is removed when the session
  is, and what is in it is the workings of a turn rather than anybody's work in progress, which is
  what makes this smaller than the two things keeping it would cost: the budget the file in the
  project needs, and a rewind naming every intermediate file it could not put back.
- **Confinement does not keep a program out of the session's directory.** A program that cannot
  open a temporary file fails outright, so the base of a `run` profile names the system temporary
  directory for reading and writing ([sandboxing.md](sandboxing.md)), and TRUST-14 puts this
  session's directory inside it. Narrowing that row to this one directory is not available, since
  it leaves a compiler or a package manager nowhere to write a temporary file of its own. So a
  confined stage reads what a turn left here without its plan naming the path, and a file it
  writes here is answered by what was said about the workspace (TRUST-16). What keeps TRUST-16's
  grounds standing after the first instant is the ownership and the mode, and neither tells a
  program this session started from the session itself. A program `run` starts is unconfined, so
  what this costs arrives with the profile rather than before it.
- **Confinement is decided before an operation runs, not while it runs.** Where a path lands is
  worked out by resolving it, and the operation happens after that, so a component that is a
  directory when it is resolved and a symlink when the file is opened carries the bytes with it.
  Closing that needs every component opened in turn with link-following refused, which is a
  different walk from the one that answers where a path goes. A tree already arranged to escape is
  refused; one rearranged inside that window is not.
- **A rule is keyed on the name, so one file inside the workspace can have two.** Confinement
  resolves a path to where it lands, but the record is written and read under the name the
  operation used below the open directory that name reaches. A symlink under that directory
  therefore gives one file two spellings and two rules: content written as untrusted through one is
  read back as trusted under the other, which is the round trip TRUST-4 exists to close. Keying the
  record on the destination instead is what closes it, and that is a change to every rule the map
  holds rather than to confinement.
- **A platform that spells its paths from a drive letter cannot open a directory by name.** Every
  rule is keyed under a `/`-spelled name (TRUST-18), so `/add-dir` and `/cd` both refuse a resolved
  path that is not one, which on Windows is every path there is. Windows is a platform this project
  supports and publishes binaries for, so that is a gap to close rather than a caveat to keep, and
  what closes it is one canonical key spelling on that platform's separator
  ([issue #25](https://github.com/brave/bravebot/issues/25)).
  Refusing is the closed direction of the two: admitting such a directory would key its rule under
  a name read as a path inside the project, where the answer given about the project at startup
  covers every file in it.

  A file inside the project on that platform has the same problem and no refusal to fall back on,
  since a name the workspace hands back below the root carries that platform's separator, and a name
  split on `/` alone is then one opaque segment: a rule about a directory does not cover the files
  under it, and the project's own broader answer decides them instead. Those names reach the map
  without anybody having spelled one, because what a run reports opening is recorded under them, and
  they reach a permission rule the same way, which is matched segment by segment as well, so one
  opaque segment matches nothing a rule a person wrote says about the path they wrote it for
  ([permissions.md](permissions.md) records that half). So the laundering this closes outside the
  project stays reachable inside it there, and what settles it is one canonical key spelling on that
  platform's separator, which is [issue #25](https://github.com/brave/bravebot/issues/25).
