---
id: RULE
title: propose_rule
status: normative
governs:
  - crates/agent/src/tools.rs
  - crates/agent/src/rule_approval.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Putting one standing rule to the person on the planner's own judgement, and writing it only on their yes ([RECALL-3](../recall.md#RECALL-3)). `scope` and `reason` shape the question and `text` is the rule. Where rules live and in what order they are read are [RECALL-2](../recall.md#RECALL-2); this file is the tool's own surface.

## Clauses

<a id="RULE-1"></a>
### RULE-1: the text is the planner's own words, and the question is fixed

`text` and `reason` are content. They are released only while the planner's context is trusted, because a rule is trusted, and a context that has fallen is refused before anything is asked ([RECALL-8](../recall.md#RECALL-8)). The schema has no `_ref` field, so a quarantined slot cannot become a rule. The question shows the text, the file the scope names and the reason. Its two choices, "Keep this rule" and "Don't keep it", are written by the driver and never by the planner. A reply the person types is their own wording and replaces the text. Anything else, a skipped question included, writes nothing. Where the text or the reason came from a private context, the question says so and offers only "Don't keep it" beside typing: the typed words are written with `why: typed by the person` and the planner's reason is dropped, since a rule is read as public in every later turn ([RECALL-8](../recall.md#RECALL-8)).

`verified-by: bravebot_agent::tools::propose_rule_offers_no_reference_field`
`verified-by: bravebot_agent::tools::a_context_that_met_something_untrusted_is_refused_before_asking`
`verified-by: bravebot_agent::tools::the_question_shows_the_text_the_file_and_the_reason`
`verified-by: bravebot_agent::tools::an_edited_rule_keeps_the_persons_words`
`verified-by: bravebot_agent::tools::a_refused_rule_writes_nothing`
`verified-by: bravebot_agent::tools::a_private_context_offers_no_keep_and_only_typed_words_write`
`verified-by: bravebot_agent::tools::a_public_context_still_offers_keep_edit_and_refuse`
`verified-by: bravebot_agent::rule_approval::only_the_first_choice_and_typed_words_write`

<a id="RULE-2"></a>
### RULE-2: a rule is one line, appended after a backup, and an answer is never reused

The line is `- <text> (<date>) why: <reason>`, with the text and the reason each flattened to one line, so no newline in either can start a second rule line. It is appended to `~/.bravebot/AGENTS.md` for `home` and to the project's `rules.md` for `project`, which makes the project's directory and its `id` file as a fact's first write does. A file that ends mid-line is not run on to. Each file is backed up before the write, so the session's undo takes the rule back; `AGENTS.md` is the one file outside the fact and rule stores that undo will put back, and only that name in the state directory itself. Each question is asked afresh, so a yes is never answered from an earlier one ([ASK-8](ask-user.md#ASK-8)).

`verified-by: bravebot_agent::tools::an_approved_rule_is_appended_with_date_and_why`
`verified-by: bravebot_agent::tools::a_project_rule_goes_to_the_projects_rules_file_beside_its_id`
`verified-by: bravebot_agent::tools::a_newline_in_a_rule_cannot_forge_a_second_rule_line`
`verified-by: bravebot_agent::tools::a_rule_follows_a_file_that_does_not_end_in_a_newline`
`verified-by: bravebot_agent::tools::saving_a_rule_asks_the_workspace_to_keep_what_it_replaces`
`verified-by: bravebot_agent::tools::a_rule_answer_is_never_reused`
`verified-by: bravebot_agent::rewind::undo_takes_back_a_rule_appended_to_the_home_agents_file`
`verified-by: bravebot_agent::rewind::no_other_file_under_the_state_directory_is_put_back_beside_the_home_agents_file`

<a id="RULE-3"></a>
### RULE-3: only the turn a person is watching can propose, and a session that writes nothing proposes nothing

A delegate is offered no `propose_rule`, and a call from one is answered as an unknown name is. A session that writes nothing under the state directory answers that it wrote nothing, before anyone is asked. Where nobody can be asked, as in a one-shot run, the question gets no answer and nothing is written ([PROMPT-9](../prompting.md#PROMPT-9)). No permission mode answers it.

`verified-by: bravebot_agent::tools::a_delegate_cannot_propose_a_rule`
`verified-by: bravebot_agent::tools::an_incognito_session_proposes_no_rule`
`verified-by: bravebot_agent::tools::a_one_shot_run_refuses_a_rule`
`verified-by: bravebot_agent::tools::no_permission_mode_answers_a_rule`
