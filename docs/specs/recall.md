---
id: RECALL
title: What a person's sessions remember, as rules and as facts
status: proposed
governs:
  - crates/agent/src/recall.rs
  - crates/agent/src/recall/fact.rs
  - crates/agent/src/recall/project.rs
  - crates/agent/src/recall/rank.rs
  - crates/agent/src/recall/store.rs
  - crates/agent/src/recall/overview.rs
  - crates/agent/src/rule_approval.rs
  - crates/agent/src/home.rs
  - crates/agent/src/preamble.rs
  - crates/agent/src/tools.rs
  - crates/agent/src/tools/recall.rs
  - crates/agent/src/tools/offered.rs
  - crates/agent/src/workspace.rs
  - crates/agent/src/rewind.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/setup.rs
  - crates/agent/src/turn/prompts.rs
  - crates/core/src/policy.rs
  - crates/core/src/delegate.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/app/actions.rs
  - crates/tui/src/command_table.rs
  - crates/tui/src/state.rs
documented-by:
  - docs/website/docs/customize/memory.md
---

## Scope

What a person's sessions keep from one session to the next, across every project, and how it gets back into a turn. There are two kinds. A **rule** is something the person said should always hold, such as "commits in this repository go out as nisfeb". A **fact** is something worth knowing later, such as "the forge polls main every 15 minutes".

Every clause here is built and pinned by tests. The spec stays `proposed` because the thresholds await the eval in Not decided.

A definition's own memory is [definition-memory.md](definition-memory.md) and is unchanged. That memory belongs to a named definition and lives in its checkout. This one belongs to the person and lives in `~/.bravebot`. Which instruction files are read, and in what order, is [instructions.md](instructions.md), which this spec extends at one place ([RECALL-2](#RECALL-2)). Labels are [labels.md](labels.md).

The design is carried over from lattice, a memory store several agents share. Its ranking, its thresholds and its rules for duplicates and supersession were measured on a real store of about 700 entries. Where a number below comes from lattice, it says so, and [Not decided](#not-decided) says how it is checked here.

## What exists today

- `~/.bravebot/AGENTS.md` and the project's `AGENTS.md` are read every turn and go into the system prompt ([INSTR-1](instructions.md#INSTR-1), [INSTR-5](instructions.md#INSTR-5), [INSTR-7](instructions.md#INSTR-7)). A rule for every project already has a home there. A rule for one project has a home only when the project has an `AGENTS.md`, and putting one there writes into a checkout that may not be the person's to change.
- A definition with `memory: project` or `local` keeps one file in its checkout, which the run must read itself ([MEMORY-4](definition-memory.md#MEMORY-4)). There is no memory at the user level, no ranking, and nothing about whether a remembered thing is still true. `memory: user` keeps nothing, because no tool writes into `~/.bravebot` without a person opening it (definition-memory.md, Open questions). [RECALL-5](#RECALL-5) is that tool, for the person's memory rather than a definition's.
- Past sessions can be searched by substring, newest first ([SESSION-33](sessions.md#SESSION-33)). Nothing is carried into a new session unless the person asks.

## Clauses

<a id="RECALL-1"></a>
### RECALL-1: a project is named by its `origin` remote once the workspace is trusted, and by its path until then

In a workspace the person has trusted at the startup question ([TRUST-7](trust-map.md#TRUST-7), [TRUST-26](trust-map.md#TRUST-26)), a project's id is the repository root's `origin` remote URL with the scheme, credentials and a trailing `.git` taken off, so `git@github.com:brave/bravebot.git` and `https://github.com/brave/bravebot` are one project. A linked worktree or a submodule, whose `.git` is a file, is named by the `origin` of the git directory its `gitdir:` line names, or of the one that directory's `commondir` names where it has one. In a workspace not trusted, in one with no `origin`, or outside a repository, the id is the canonical path of the repository root, or of the working directory when there is no repository. The id is resolved again before every turn, so it follows `/cd` ([TRUST-13](trust-map.md#TRUST-13)) and a yes at the startup question. A delegate's checkout ([CHECKOUT-6](checkouts.md#CHECKOUT-6)) has the id of the session that made it.

The id becomes a directory under `~/.bravebot/projects/` named by a hash of the id, with the id written in a file beside it, so two ids never share a directory and the person can see which is which. The directory is made by the first write that needs it ([RECALL-3](#RECALL-3), [RECALL-5](#RECALL-5)), never by a read.

The first time the workspace is trusted, what was kept under its path moves to the `origin` name, so the facts and rules saved before the yes keep loading. Only where the `origin` name has nothing kept yet: nothing is merged or written over, and where both are kept the path's stays where it is. A session that writes nothing to the state directory moves nothing.

**Why not the path.** The session store names a directory by a lossy mangling of the path, and its known costs say two paths can share a store. Rules must not leak between projects that way. Two clones of one repository should share their rules, and moving a checkout should not lose them.

**Why the path until trusted.** `.git/config` is a project file. Choosing which project's rules and facts to load from it is a decision taken from content nobody has vouched for ([LABEL-5](labels.md#LABEL-5)). A hostile repository could name another project's `origin` and have that project's rules and private facts put in front of the planner. Until the yes, a workspace gets only the home layer and the facts kept for its path.

`verified-by: bravebot_agent::recall::project::the_two_spellings_of_one_remote_are_one_project`
`verified-by: bravebot_agent::recall::project::a_remote_that_names_no_host_names_no_project`
`verified-by: bravebot_agent::recall::project::credentials_in_a_remote_are_dropped`
`verified-by: bravebot_agent::recall::project::an_untrusted_workspace_is_named_by_its_path`
`verified-by: bravebot_agent::recall::project::a_trusted_workspace_is_named_by_its_origin`
`verified-by: bravebot_agent::recall::project::a_trusted_repository_with_no_origin_is_named_by_its_path`
`verified-by: bravebot_agent::recall::project::a_tracked_config_beside_a_git_file_does_not_name_the_project`
`verified-by: bravebot_agent::recall::project::a_bare_repository_is_named_by_its_origin`
`verified-by: bravebot_agent::recall::project::what_was_kept_under_the_path_moves_to_the_origin_once_trusted`
`verified-by: bravebot_agent::recall::project::a_linked_worktree_is_named_by_its_repositorys_origin`
`verified-by: bravebot_agent::recall::project::a_directory_outside_a_repository_is_named_by_its_path`
`verified-by: bravebot_agent::recall::project::the_directory_name_is_sixteen_hex_of_the_ids_hash`
`verified-by: bravebot_agent::turn::a_turn_with_only_home_rules_makes_no_projects_directory`

<a id="RECALL-2"></a>
### RECALL-2: rules come from three layers, and every layer is loaded in full every turn

The layers, read in this order:

| Layer | File | Applies to |
|---|---|---|
| home | `~/.bravebot/AGENTS.md` | every project |
| person, per project | `~/.bravebot/projects/<id>/rules.md` | the project [RECALL-1](#RECALL-1) names |
| project | the project's `AGENTS.md` and its fallbacks ([INSTR-1](instructions.md#INSTR-1)) | that project |

`rules.md` is one of the sources in [INSTR-1](instructions.md#INSTR-1), the second, read on the same footing as `~/.bravebot/AGENTS.md`: trusted for sitting in `~/.bravebot` ([TRUST-11](trust-map.md#TRUST-11)), re-read every turn with no cache, and put in the system prompt, never the conversation. Delegates read it as they read the other standing instructions ([INSTR-10](instructions.md#INSTR-10)). Where the person's file and the project's `AGENTS.md` disagree, the project's file wins, and the system prompt says so in one line above the rules. Sources that come later still, nested `AGENTS.md` files ([INSTR-14](instructions.md#INSTR-14)) and words from the command line ([INSTR-10](instructions.md#INSTR-10)), keep their place after both.

A rule is never dropped to save tokens. A layer is loaded in full, or the session says it could not be read. Its size is counted where [INSTR-1](instructions.md#INSTR-1)'s sources are already measured.

**Why no ranking for rules.** A ranked recall can miss, and a rule the person set must never go missing, so a rule does not go through recall at all.

`verified-by: bravebot_agent::preamble::the_persons_rules_for_a_project_come_between_home_and_project`
`verified-by: bravebot_agent::preamble::the_header_says_the_projects_file_wins_over_the_persons_rules`
`verified-by: bravebot_agent::preamble::an_untrusted_workspace_reads_the_rules_kept_for_its_path`
`verified-by: bravebot_agent::preamble::reading_rules_makes_no_projects_directory`
`verified-by: bravebot_agent::preamble::a_delegates_checkout_reads_its_parents_rules`
`verified-by: bravebot_agent::preamble::a_cd_to_another_project_reads_that_projects_rules`
`verified-by: bravebot_agent::preamble::a_rules_file_is_read_again_by_the_next_turn`
`verified-by: bravebot_agent::preamble::rules_inside_the_working_directory_are_not_read_as_the_persons_own`
`verified-by: bravebot_agent::preamble::rules_that_cannot_be_read_are_said_not_to_have_been_read`
`verified-by: bravebot_agent::preamble::rules_in_a_folder_that_cannot_be_searched_are_said_not_to_have_been_read`

<a id="RECALL-3"></a>
### RECALL-3: a rule is written only after the person's yes, and only from words the planner wrote

The planner proposes a rule with the `propose_rule` tool, giving its text, its scope (home or this project) and a one-line reason. The person is asked ([prompting.md](prompting.md)) with the text, the scope and the reason shown, and can approve it, edit it or refuse it. Only an approval writes. Where the interface lists the choices and reads a typed line, a line that is a choice's label, its number or a yes or a no is that choice, and only other text is the person's own wording of the rule. The rule is appended to `~/.bravebot/AGENTS.md` or to the project's `rules.md` as one line: the text, the date, and `why:` with the reason. A rule proposed from words that came from private material is not written as proposed: the question offers only refusing or typing the rule in the person's own words, and what is written is those words with `why: typed by the person`, never the planner's reason ([RECALL-8](#RECALL-8)). Each write is backed up first, so `/undo` takes the rule back. The person can edit either file by hand.

`propose_rule` takes literal text from the planner's own context, which holds nothing untrusted ([LABEL-3](labels.md#LABEL-3), [LABEL-9](labels.md#LABEL-9)). It takes no reference to a data slot. A fact never becomes a rule by promotion. The person restates it, or the planner proposes it in its own words and the person approves.

A delegate cannot propose a rule. No permission mode answers the question. Where nobody can be asked, as in a one-shot run, the proposal is refused ([PROMPT-9](prompting.md#PROMPT-9)). In a background session it waits for a person, as a held prompt does ([BG-7](background-sessions.md#BG-7)). The question has its row in the register ([PROMPT-11](prompting.md#PROMPT-11)): the answer is kept only as the line it writes. An incognito session can propose no rule.

`verified-by: bravebot_agent::tools::an_approved_rule_is_appended_with_date_and_why`
`verified-by: bravebot_agent::tools::a_project_rule_goes_to_the_projects_rules_file_beside_its_id`
`verified-by: bravebot_agent::tools::a_refused_rule_writes_nothing`
`verified-by: bravebot_agent::tools::an_edited_rule_keeps_the_persons_words`
`verified-by: bravebot_agent::rule_approval::a_typed_reply_that_names_a_choice_is_that_choice`
`verified-by: bravebot_agent::tools::a_newline_in_a_rule_cannot_forge_a_second_rule_line`
`verified-by: bravebot_agent::tools::a_context_that_met_something_untrusted_is_refused_before_asking`
`verified-by: bravebot_agent::tools::a_one_shot_run_refuses_a_rule`
`verified-by: bravebot_agent::tools::a_delegate_cannot_propose_a_rule`
`verified-by: bravebot_agent::tools::an_incognito_session_proposes_no_rule`
`verified-by: bravebot_agent::tools::a_rule_answer_is_never_reused`
`verified-by: bravebot_agent::tools::a_private_context_offers_no_keep_and_only_typed_words_write`
`verified-by: bravebot_agent::tools::no_permission_mode_answers_a_rule`
`verified-by: bravebot_agent::tools::propose_rule_offers_no_reference_field`
`verified-by: bravebot_agent::rewind::undo_takes_back_a_rule_appended_to_the_home_agents_file`

<a id="RECALL-4"></a>
### RECALL-4: a fact is one file, and what it says about itself is at its top

A fact for one project is `~/.bravebot/projects/<id>/facts/<key>.md`. A fact for every project is `~/.bravebot/facts/<key>.md`. A key is a slug path such as `release/forge-poll`. The file starts with front matter:

| Field | Holds |
|---|---|
| `source` | `person` when the person asked for it to be remembered, `agent` otherwise |
| `label` | `public` or `private`, from [RECALL-8](#RECALL-8) |
| `created`, `verified` | dates; a save sets `verified` to today and keeps an earlier `created` |
| `superseded-by` | the key of the fact that replaced it, when one did |
| `session` | the session that saved it |
| `tags` | free words, used by recall |

A file with no front matter, or with a field missing, reads as `source: agent`, `label: private` and no dates. A fact holds one thing.

**Why front matter and not a database.** The fields travel with the file when it is moved, copied or edited by hand, and an older build reads them as text. Lattice keeps provenance the same way for the same reason.

`verified-by: bravebot_agent::recall::fact::a_fact_round_trips_through_its_file`
`verified-by: bravebot_agent::recall::fact::a_file_with_no_front_matter_is_an_agent_fact_kept_private`
`verified-by: bravebot_agent::recall::fact::an_unclosed_front_matter_is_read_as_body`
`verified-by: bravebot_agent::recall::fact::a_key_is_a_slug_path`
`verified-by: bravebot_agent::recall::store::a_fact_is_written_where_its_scope_says`
`verified-by: bravebot_agent::recall::store::a_nested_key_is_written_and_loaded`
`verified-by: bravebot_agent::recall::store::loading_makes_no_directory`
`verified-by: bravebot_agent::recall::store::a_file_that_is_not_text_is_named_and_skipped`
`verified-by: bravebot_agent::tools::a_fact_is_saved_and_its_line_shown`
`verified-by: bravebot_agent::tools::a_project_fact_is_kept_under_the_project`
`verified-by: bravebot_agent::tools::a_text_that_looks_like_front_matter_is_only_a_body`
`verified-by: bravebot_agent::tools::a_session_id_with_a_control_character_is_not_written`

<a id="RECALL-5"></a>
### RECALL-5: the planner saves a fact on its own judgement, and every save is shown

The planner saves with the `remember` tool, giving a key, a scope (every project or this one) and the text. The text is literal text from the planner's own context. `remember` takes no reference to a data slot. The save is shown in the transcript as one line naming the key and the first line of the text. It is a step the session's `/undo` and `/rewind` take apart like a file write: undoing it removes the file it made, or puts back the one it replaced. A rewind point holds the replaced bytes, as it does for any write ([SESSION-22](sessions.md#SESSION-22)). The session record, which programs may read, keeps a replaced fact's bytes only where that fact was labelled public, and a rules file's as it keeps any other; a private fact's save is undone in its own session and not after a resume. A session record read back believes a backup's mark as one of these stores only for a path in `~/.bravebot/facts/` or `~/.bravebot/projects/`, or for `~/.bravebot/AGENTS.md`; any other path so marked comes back as not kept. Where `~/.bravebot/AGENTS.md` is a link, as a dotfiles checkout keeps it, a rule's backup is taken of the file the link lands on, and undo puts it back only while the link still lands there.

`remember` works in every permission mode, plan mode included, since it writes nothing in the workspace. A delegate cannot save. An incognito session saves nothing, and `remember` answers that it kept nothing. A one-shot run saves, and its output names each fact it kept.

Nothing is saved without the tool. The driver never pulls a fact out of a conversation on its own.

**Why a tool of its own.** [MEMORY-6](definition-memory.md#MEMORY-6) gives a definition's memory no tool, because a tool that wrote only the memory would be a write gated on which file it is. These files are not in the workspace and no other tool reaches them ([RECALL-8](#RECALL-8)), so `remember` is not a gate on a path but the only way in. A save is a write into `~/.bravebot` that no person opens, so each one is shown and can be undone.

`verified-by: bravebot_agent::tools::a_fact_is_saved_and_its_line_shown`
`verified-by: bravebot_agent::tools::a_delegate_cannot_save_a_fact`
`verified-by: bravebot_agent::tools::a_delegate_reaching_the_handler_anyway_is_refused`
`verified-by: bravebot_agent::tools::an_incognito_session_keeps_no_fact`
`verified-by: bravebot_agent::tools::remember_works_in_plan_mode`
`verified-by: bravebot_agent::tools::remember_offers_no_reference_field`
`verified-by: bravebot_agent::tools::a_bad_key_is_refused`
`verified-by: bravebot_agent::tools::saving_asks_the_workspace_to_keep_what_it_replaces`
`verified-by: bravebot_agent::turn::a_save_is_undone_by_undo`
`verified-by: bravebot_agent::rewind::undo_removes_a_fact_a_turn_made`
`verified-by: bravebot_agent::rewind::undo_puts_back_a_fact_a_turn_replaced`
`verified-by: bravebot_agent::rewind::a_kept_backup_outside_the_two_stores_is_not_put_back`
`verified-by: bravebot_session::sessions::a_kept_mark_on_a_path_outside_the_stores_drops_the_backup`
`verified-by: bravebot_session::sessions::overwriting_a_private_fact_leaves_no_fact_bytes_in_the_saved_record`
`verified-by: bravebot_agent::rewind::a_differently_cased_store_name_is_not_put_back_on_a_case_sensitive_volume`
`verified-by: bravebot_agent::rewind::a_rule_through_a_linked_home_rules_file_is_undone_only_while_the_link_lands_there`
`verified-by: bravebot_session::sessions::a_kept_mark_on_the_file_a_linked_home_rules_file_lands_on_is_believed`

<a id="RECALL-6"></a>
### RECALL-6: a new fact too close to an existing one is refused, and the refusal names it

A save under a new key is compared with the live facts in its scope by the share of its terms they hold. At 60% or more the save is refused, naming the closest fact, unless the planner passes `new: true`. Every save's answer lists facts above 45%. The planner either saves over the named fact or supersedes it ([RECALL-7](#RECALL-7)). Every fact compared was written by `remember` or by the person, so the comparison reads only trusted text.

**Where the numbers come from.** On lattice's store, true duplicates scored 60% or more and distinct pairs under 45%.

`verified-by: bravebot_agent::tools::a_fact_close_to_a_kept_one_is_refused_naming_it`
`verified-by: bravebot_agent::tools::saving_over_a_kept_key_is_not_a_duplicate`
`verified-by: bravebot_agent::tools::new_true_saves_past_the_refusal`
`verified-by: bravebot_agent::tools::every_save_lists_facts_above_45`
`verified-by: bravebot_agent::recall::rank::term_share_of_near_duplicates_is_over_60`

<a id="RECALL-7"></a>
### RECALL-7: a fact that stops being true is superseded, not deleted

`remember` with `supersedes: <key>` saves the new fact and sets `superseded-by` on the old one. Recall leaves superseded facts out. Reading one with `recall` shows its text and the key that replaced it. Deleting is the person's, by hand or by `/undo`.

`verified-by: bravebot_agent::tools::superseding_marks_the_old_fact`
`verified-by: bravebot_agent::tools::superseding_a_fact_that_is_not_kept_saves_nothing`
`verified-by: bravebot_agent::tools::recall_leaves_superseded_facts_out_of_a_query`
`verified-by: bravebot_agent::tools::reading_a_superseded_fact_by_key_names_its_replacement`

<a id="RECALL-8"></a>
### RECALL-8: only `remember` and the person write a fact, and a fact keeps the confidentiality of what it was written from

A store's name is matched ignoring case, so a differently cased spelling is refused too, on a volume that tells the two apart as well as on one that does not: it reads as the store, and nothing that belongs in a workspace is called that. `/undo` restores only the store itself and asks the volume. The fact directories are refused to every other tool and to every program, as the rest of the state directory is ([SANDBOX-12](sandboxing.md#SANDBOX-12), [STATE-3](state-directory.md#STATE-3)). A `run` line's `<` redirection is opened by bravebot rather than by a program, so its source is refused where it lands in them, before the line runs and again as it is opened. The git tool reads files from the object store and not through the file tools, so it leaves them out of what it shows and says a file was left out. A workspace that holds the state directory, as one rooted at the home directory does, cannot reach `~/.bravebot/facts/`, `~/.bravebot/projects/`, `~/.bravebot/AGENTS.md` or `~/.bravebot/sessions/` with the file tools either, and a walk over it finds nothing there. The planner's context holds nothing untrusted, and `remember` takes only text from it, so no fact holds untrusted bytes. A file the person puts there by hand is theirs, as `~/.bravebot/AGENTS.md` is.

A fact is trusted. Its confidentiality is the planner context's at the moment of the save ([LABEL-2](labels.md#LABEL-2)), joined with the old fact's where the save writes over one, since its tags and date carry over, and written as `label`. A fact read into a turn, by injection or by `recall`, gives the context that label, as any read of private content does, so private content cannot reach a public surface by being remembered. This is a new row in [LABEL-8](labels.md#LABEL-8)'s table: a fact's first label is trusted, with the confidentiality its `label` field states, and private where the field is missing.

**Why.** A site must not get a fact into memory. Untrusted content reaches only processors and data slots. Refusing slots as `remember`'s text and refusing every other writer keeps it out without the trust map, which does not govern `~/.bravebot` ([TRUST-11](trust-map.md#TRUST-11)).

`verified-by: bravebot_agent::workspace::a_workspace_holding_home_cannot_write_a_fact_with_the_file_tools`
`verified-by: bravebot_agent::workspace::a_workspace_holding_home_cannot_write_the_home_rules_or_reach_the_sessions`
`verified-by: bravebot_agent::workspace::a_workspace_holding_home_can_still_write_its_other_files`
`verified-by: bravebot_agent::workspace::a_link_inside_the_workspace_into_the_fact_store_is_refused`
`verified-by: bravebot_agent::workspace::a_walk_over_a_workspace_holding_home_finds_nothing_in_the_stores`
`verified-by: bravebot_agent::workspace::a_rewind_cannot_reach_the_fact_store`
`verified-by: bravebot_agent::workspace::a_differently_cased_store_name_is_refused_before_the_store_exists`
`verified-by: bravebot_agent::workspace::a_differently_cased_store_name_is_refused_whatever_the_volume_says`
`verified-by: bravebot_sandbox::base::programs_are_refused_the_fact_store`
`verified-by: bravebot_agent::tools::a_redirection_reading_from_the_stores_is_refused_before_it_runs`
`verified-by: bravebot_agent::workspace::git_leaves_the_stores_out_of_a_repository_that_tracks_them`
`verified-by: bravebot_agent::rewind::undo_removes_a_fact_a_turn_made`
`verified-by: bravebot_agent::rewind::undo_puts_back_a_fact_a_turn_replaced`
`verified-by: bravebot_agent::rewind::a_kept_backup_outside_the_two_stores_is_not_put_back`
`verified-by: bravebot_agent::rewind::a_differently_cased_store_name_is_not_put_back_on_a_case_sensitive_volume`
`verified-by: bravebot_core::policy::a_kept_fact_is_trusted_with_the_confidentiality_it_states`
`verified-by: bravebot_core::policy::labelling_a_kept_fact_is_recorded_in_the_audit_trail`
`verified-by: bravebot_core::policy::reading_a_private_fact_makes_the_context_private`
`verified-by: bravebot_core::policy::reading_a_public_fact_leaves_the_context_as_it_was`
`verified-by: bravebot_core::policy::the_planners_words_are_kept_as_a_fact_only_while_the_context_is_trusted`
`verified-by: bravebot_core::policy::a_trusted_value_is_not_kept_as_a_fact_once_the_context_has_fallen`
`verified-by: bravebot_agent::tools::a_refusal_naming_a_private_fact_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_listing_of_a_private_fact_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_fact_saved_from_a_private_context_is_private`
`verified-by: bravebot_agent::tools::saving_over_a_private_fact_from_a_public_context_keeps_it_private`
`verified-by: bravebot_agent::tools::a_fact_from_a_context_that_met_something_untrusted_is_refused`
`verified-by: bravebot_agent::tools::remember_offers_no_reference_field`

<a id="RECALL-9"></a>
### RECALL-9: at most three facts reach a prompt unasked, each over two floors, and each is marked as background

Before a planned turn, the person's prompt is ranked against the project's facts and the home facts ([RECALL-10](#RECALL-10)). At most three are put in the turn's system prompt, each needing a strength of 50 or more and a match on at least two distinct terms of the prompt. They go in a block marked as background, not instructions, with each fact's key and `verified` date. They are never written into the stored conversation. The keys injected are kept in the session record, so a fact is injected once per session, across resume. `/clear` empties that set. A prompt under three words, or a slash command, injects nothing. A delegate gets no injected facts. A key resolves to the project's own fact first, so an everywhere fact under the same key is not injected beside it.

**Why two floors.** Strength alone let a short prompt match a fact fully on one common word. Lattice added a floor on the raw score for that and, on 16 real prompts, cut injections from 12 to 4 while keeping the right fact for 34 of 35 eval questions. A raw BM25 score moves with the size of the store, so the floor here is a count of terms matched, which does not.

`verified-by: bravebot_agent::recall::a_short_prompt_injects_nothing`
`verified-by: bravebot_agent::recall::a_slash_command_injects_nothing`
`verified-by: bravebot_agent::recall::one_common_word_is_not_enough`
`verified-by: bravebot_agent::recall::a_project_fact_shadows_an_everywhere_fact_under_its_key`
`verified-by: bravebot_agent::recall::one_compound_word_is_not_enough`
`verified-by: bravebot_agent::recall::at_most_three_are_injected`
`verified-by: bravebot_agent::recall::a_fact_is_injected_once_per_session`
`verified-by: bravebot_agent::recall::a_replaced_fact_is_not_injected`
`verified-by: bravebot_agent::turn::injected_facts_go_in_the_system_prompt_not_the_conversation`
`verified-by: bravebot_agent::turn::a_delegate_gets_no_injected_facts`
`verified-by: bravebot_agent::turn::a_turn_is_labelled_by_the_facts_injected_into_it`
`verified-by: bravebot_agent::turn::a_prompt_that_matches_nothing_leaves_the_turn_public`
`verified-by: bravebot_session::sessions::the_injected_set_survives_resume`
`verified-by: bravebot_session::sessions::an_old_record_reads_with_no_injected_set`
`verified-by: bravebot_agent::turn::a_failed_turn_still_reports_what_it_injected`
`verified-by: bravebot_agent::turn::a_prompt_the_driver_wrote_or_composed_injects_nothing`
`verified-by: bravebot_agent::turn::a_body_cannot_start_a_line_of_its_own_and_the_block_is_closed`
`verified-by: bravebot_tui::app::clearing_empties_the_injected_set`

<a id="RECALL-10"></a>
### RECALL-10: recall ranks by BM25, says when probably nothing is stored, and is a tool too

Ranking is BM25 (k1 1.2, b 0.75) over the key counted twice, the tags and the body, never the front matter. Terms are lower-cased, split at punctuation with the whole word also kept, stop words dropped, and lightly stemmed. A quoted phrase must appear as written. A fact's **strength** is the share of the query's IDF weight it matches. When the best result is under 35 the answer says `weak`, meaning probably nothing is stored, and the planner is told to say so rather than assume.

The planner can call `recall` with a query for more than [RECALL-9](#RECALL-9) injects. Its results include, for the top three, the facts their bodies link to with `[[key]]`, one hop and at most five, each marked with the fact it came from.

A query answers with at most eight live facts, from the project's facts and the home facts together, each with its key, strength and `verified` date followed by its body. A key reads that one fact, replaced or not. Every fact an answer names, by key, strength, date, link or text, is read through the fact's `label` and the answer is held at the join of them ([RECALL-8](#RECALL-8)); a query's answer takes the join over every live fact it ranked, shown or not. It is not offered to a delegate, and is specified in [tools/recall.md](tools/recall.md).

`verified-by: bravebot_agent::recall::rank::a_compound_gives_the_whole_word_and_each_part`
`verified-by: bravebot_agent::recall::rank::stop_words_are_dropped_and_words_lightly_stemmed`
`verified-by: bravebot_agent::recall::rank::the_key_counts_twice`
`verified-by: bravebot_agent::recall::rank::strength_is_the_share_of_idf_matched`
`verified-by: bravebot_agent::recall::rank::a_quoted_phrase_must_appear_as_written`
`verified-by: bravebot_agent::recall::rank::matched_terms_counts_distinct_query_terms`
`verified-by: bravebot_agent::recall::rank::text_with_no_spaces_or_only_punctuation_ranks_without_panic`
`verified-by: bravebot_agent::recall::rank::links_reads_both_forms`
`verified-by: bravebot_agent::tools::recall_finds_a_fact_by_its_words`
`verified-by: bravebot_agent::tools::recall_says_weak_when_nothing_matches`
`verified-by: bravebot_agent::tools::recall_follows_links_one_hop_from_the_top_three`
`verified-by: bravebot_agent::tools::recall_reads_the_project_and_every_project`
`verified-by: bravebot_agent::tools::recall_answers_at_most_the_result_count`
`verified-by: bravebot_agent::tools::recalling_a_private_fact_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_private_fact_linked_to_makes_the_context_private`
`verified-by: bravebot_agent::tools::a_delegate_cannot_recall`

<a id="RECALL-11"></a>
### RECALL-11: `/memory` shows rules and facts with their standing

`/memory` adds two sections to what [MEMORY-12](definition-memory.md#MEMORY-12) lists: the rule layers a turn here reads, by the same tests the turn applies (so none in a safe session, and a link is followed where the turn follows it), with each file's path and line count, and the facts in scope, each with its key, `source`, `label`, `verified` date and whether it is live or superseded. It reads the front matter of fact files, which are trusted ([RECALL-8](#RECALL-8)). It still reads no definition's memory.

`verified-by: bravebot_tui::app::the_memory_listing_shows_rules_and_facts_with_their_standing`
`verified-by: bravebot_agent::recall::overview::a_linked_rules_file_is_listed_as_a_turn_reads_it_and_a_pipe_is_not_read`
`verified-by: bravebot_agent::recall::overview::a_long_rules_file_is_counted_up_to_a_cap_and_the_working_directorys_own_is_listed`
`verified-by: bravebot_agent::agents::a_pointer_lists_its_target`
`verified-by: bravebot_agent::agents::a_file_a_deny_rule_covers_is_not_listed`
`verified-by: bravebot_agent::safe::a_safe_session_lists_no_project_rules_file`
`verified-by: bravebot_agent::safe::a_safe_session_lists_no_state_rule_layers`
`verified-by: bravebot_tui::app::hostile_front_matter_is_drawn_as_harmless_text`

## What this changes in other specs

- [instructions.md](instructions.md): [INSTR-1](instructions.md#INSTR-1) gains `~/.bravebot/projects/<id>/rules.md` as its second source, after `~/.bravebot/AGENTS.md` and before the project's files. The measured sources gain it too.
- [labels.md](labels.md): [LABEL-8](labels.md#LABEL-8)'s table gains the row in [RECALL-8](#RECALL-8).
- [prompting.md](prompting.md): [PROMPT-11](prompting.md#PROMPT-11)'s register gains `propose_rule`, and the moments a person is asked gain it.
- [incognito.md](incognito.md): an incognito session saves no fact, proposes no rule, and makes no `projects/<id>/` directory. It still reads rules and facts, so the session is the one the person configured.
- [definition-memory.md](definition-memory.md): [MEMORY-12](definition-memory.md#MEMORY-12)'s `/memory` gains the two sections in [RECALL-11](#RECALL-11).
- [state-directory.md](state-directory.md): `~/.bravebot/projects/` and `~/.bravebot/facts/` join the state directory, 0600 like the rest, and refused to programs like the rest.

## Not decided

- **Whether lattice's numbers hold here.** The thresholds (35, 45, 50, 60, two terms) were measured, or in the last case reasoned, on one person's store of short notes. Before this is normative, a private eval of real questions against a real bravebot store should check them, as lattice did with 42 questions. Until then they are defaults kept in one place in the code.
- **Importing.** [import.md](import.md) could bring in a Claude Code `MEMORY.md` and its files as facts. Whether they come in as `source: person`, and with which label, is open.
- **Moving a fact between scopes.** Moving a fact from one project to every project is a hand move of the file today.

## Known costs

- BM25 matches words, not meaning. A fact about "the forge" is not found by a prompt about "CI".
- A fresh clone the person has not yet trusted gets no per-project rules or facts ([RECALL-1](#RECALL-1)). A rule such as "use tabs here" is missing there until the yes. What was kept under the path moves to the `origin` name at the yes, except where the `origin` already had its own, which is left for the person to merge by hand.
- A repository with no `origin` is named by its path, so moving it loses its rules and facts until the person moves the directory under `~/.bravebot/projects/`.
- A fork with its own `origin` is a different project from its upstream.
- The planner saving on its own judgement will save things the person would not have. The shown line and `/undo` are the check, and `/memory` shows what is kept.
- Most planner contexts are private, so most facts will be too, and a turn that reads one becomes private. That is the safe side, and the cost is fewer surfaces open to that turn.
- Which public facts are injected depends on the term statistics of every fact ranked, private ones included. The query is the person's prompt, which the planner neither chooses nor repeats, so this is not a probe, and a turn is labelled only by the facts injected into it.
- Text in a script written without spaces, such as Chinese or Japanese, ranks as whole runs of characters, so a query that holds only part of a run does not match it.
- In a repository whose root holds the home directory, `git status` and a shell's glob expansion can show the names of files under the stores, never their contents. Where the repository tracks them, as a dotfiles repository may, the git tool leaves them out of what it shows and says a file was left out.
- Rules are loaded in full however long the files grow. A long `rules.md` costs tokens on every turn, and the size shows where [INSTR-1](instructions.md#INSTR-1)'s sources are measured.
