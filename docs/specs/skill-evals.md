---
id: SKILLEVAL
title: Evaluating a skill
status: normative
governs:
  - crates/agent/src/skill_eval.rs
  - crates/cli/src/skill_eval.rs
documented-by:
  - docs/website/docs/reference/cli.md
  - docs/website/docs/customize/skills.md
---

## Scope

`bravebot skill eval <name>` runs the saved cases of one skill with the skill and again without it,
grades both, and reports two scores and their difference. It is for the author of a skill, who
otherwise has no number to compare before and after a change to a description or a body, and no way
to see a skill that nobody finds ([skills.md](skills.md)).

It evaluates skills of the person's own (`~/.bravebot/skills` and the shared directories their
settings name) and skills of a project the trust map vouches for. A plugin is not evaluated
([plugins.md](plugins.md) is proposed and nothing is built). Running one task on several models is
not part of it.

## Clauses

<a id="SKILLEVAL-1"></a>
### SKILLEVAL-1: `skill eval <name>` evaluates one skill the person could be offered, and refuses any other

The name is compared whole against the skills the catalogue found, read the way a turn reads them:
the person's own through their provenance and a project's through the trust map. A name that matches
no skill, and a name that matches only something in a project nobody vouched for, are refused with
the same sentence, which names the argument the person typed and nothing from the project. The
refusal ends on the status for a refused argument and nothing runs.

A skill kept from the planner by `disable-model-invocation` is refused with a sentence saying why.
A baseline without such a skill would be the same catalogue, since no turn is offered it, so both
arms would score alike whatever the skill held.

**Why.** An untrusted project can name a directory to read like an instruction, so the refusal for
it must be the one a typo gets. Refusing the person-only skill, and not skipping it, tells the
author why a command they typed did nothing.

`verified-by: bravebot_agent::skill_eval::a_project_nobody_vouched_for_offers_no_skill_to_evaluate`
`verified-by: bravebot_cli::skill_eval::a_name_nobody_can_evaluate_is_refused_with_one_sentence`
`verified-by: bravebot_cli::skill_eval::a_skill_kept_from_the_planner_is_refused_and_says_why`

<a id="SKILLEVAL-2"></a>
### SKILLEVAL-2: a skill's cases are JSON files in the `evals` directory beside its `SKILL.md`

A case holds a `prompt`, optionally `repeats` (3 where absent, at least 1 and at most 20) and at
least one grader. A grader holds a `check` and optionally a `weight` (1 where absent, at least 1).
The checks are `tool_called`, `tool_not_called`, `rounds_at_most`, `skill_loaded` and `rubric`. A
file with a key this version does not read, an empty prompt, no grader, a repeat count outside the
range or a weight of 0 is refused, and the refusal names the file, which the catalogue listed. A
key is never ignored, so a misspelt grader cannot be dropped and leave a case that passes by
checking nothing. A skill with no accepted case is refused as an argument, since there is nothing to
run.

`verified-by: bravebot_agent::skill_eval::a_case_file_reads_into_prompt_repeats_and_weighted_graders`
`verified-by: bravebot_agent::skill_eval::a_case_that_checks_nothing_or_runs_without_limit_is_refused`
`verified-by: bravebot_cli::skill_eval::a_skill_with_no_accepted_case_is_refused`

<a id="SKILLEVAL-3"></a>
### SKILLEVAL-3: a case file is read with the trust of its skill, and what was left out is counted and never named

Only files the catalogue listed beside the skill are candidates, and each is read through the
gate the skill itself was read through. A file the listing left out, because a permission rule
denies reading it or the trust map does not vouch for it, is never read. It is counted, and the
count is said without a name ([skills.md](skills.md) says the same of skills). A case file is
never put to the planner as anything but the prompt of a run the person asked for, and a name
outside the listing, such as a path with `..`, is not read.

`verified-by: bravebot_agent::skill_eval::cases_of_a_users_skill_and_of_a_vouched_project_skill_are_loaded`
`verified-by: bravebot_agent::skill_eval::a_case_file_nobody_vouched_for_is_counted_and_never_read`
`verified-by: bravebot_agent::skill_eval::a_name_the_catalogue_did_not_list_is_not_read`

<a id="SKILLEVAL-4"></a>
### SKILLEVAL-4: every case runs as many times with the skill as without it, and the arms differ by that skill alone

Each case runs its `repeats` times in each arm. In the baseline arm the catalogue is the one the
turn found with the one skill removed: the planner's search misses it, `load_skill` refuses it as it
refuses any name the catalogue does not hold, and everything else is as in the other arm, including
the other skills, hooks, instruction files, definitions and servers. The baseline is not a `--safe`
run and not a catalogue narrowed to some skills, and the catalogue is not marked curated by
removing the skill. Nothing a case or a skill file says changes what differs between the arms.

`verified-by: bravebot_agent::skills::without_removes_the_named_skill_and_only_that_one`
`verified-by: bravebot_agent::skills::without_leaves_the_catalogue_as_uncurated_as_it_was`
`verified-by: bravebot_agent::turn::a_turn_given_a_skill_to_do_without_neither_finds_nor_loads_it`
`verified-by: bravebot_cli::skill_eval::a_skills_cases_run_with_it_and_without_it_and_the_report_gives_both_scores`
`verified-by: bravebot_cli::skill_eval::both_arms_run_every_case_the_same_number_of_times`

<a id="SKILLEVAL-5"></a>
### SKILLEVAL-5: a run approves nothing, writes nothing and stops at a cap

A run is a one-shot run with nobody to ask: an effect that needs a prompt is refused, and
`skill eval` accepts none of the flags that let a run write. A refusal is in the run's record and
is counted in the report. Every run carries the limits of [CLI-38](cli.md#CLI-38): a limit on its
rounds, and a limit on its tokens where the person gave one. A run that reaches either limit is
stopped before the round or the request that would pass it, is reported as stopped, and has no
score: it is neither a pass nor a fail, and it takes no part in the mean of its case.

`verified-by: bravebot_cli::skill_eval::a_run_that_reaches_its_cap_has_no_score`
`verified-by: bravebot_cli::skill_eval::a_run_that_has_spent_max_tokens_is_stopped_and_has_no_score`
`verified-by: bravebot_cli::skill_eval::a_run_that_would_write_is_refused_and_counted`
`verified-by: bravebot_cli::skill_eval::the_command_takes_no_flag_that_lets_a_run_write`

<a id="SKILLEVAL-6"></a>
### SKILLEVAL-6: a deterministic grader reads the driver's record of the run, never the reply

`tool_called` and `tool_not_called` compare a name with the tools the run called. `rounds_at_most`
compares a count with the rounds it made. `skill_loaded` is whether the run called `load_skill` and
the call resolved to the catalogue entry of the skill under test. The entry is the one the catalogue
returned when `load_skill` was executed, recorded by its own name in the call's record, so no word
the model typed is compared: a name that differs in spelling from the entry's and resolves to it
counts, and a name that resolves to no entry does not. The report shows, beside each arm, how many
runs searched for the skill and how many loaded it, so a skill nobody finds is visible. No grader
reads the reply or the target a call was drawn with, and the tally branches on counts, names the
driver holds and statuses only.

`verified-by: bravebot_cli::skill_eval::a_deterministic_grader_reads_the_record_and_not_the_reply`
`verified-by: bravebot_cli::skill_eval::the_record_ignores_the_target_a_call_was_drawn_with`
`verified-by: bravebot_cli::skill_eval::a_skill_counts_as_loaded_only_when_the_catalogue_entry_it_resolved_to_was_answered`
`verified-by: bravebot_cli::skill_eval::a_skills_cases_run_with_it_and_without_it_and_the_report_gives_both_scores`

<a id="SKILLEVAL-7"></a>
### SKILLEVAL-7: a rubric is judged by a model with no tools, three times, and counts only where the gate releases the verdict

A `rubric` grader gives a model with no tools the exchange and the rubric, and asks it three times.
The grader passes when at least two verdicts are a pass. A verdict counts only where the gate
releases it to the driver. A verdict whose label is untrusted is counted nowhere, so it changes no
score, and a rubric that has no released verdict leaves its run without a score for that grader.
The report counts the runs whose rubric verdicts were withheld.

`verified-by: bravebot_agent::skill_eval::a_verdict_counts_as_a_pass_only_where_the_judge_said_met`
`verified-by: bravebot_agent::skill_eval::a_rubric_passes_on_two_of_three_votes`
`verified-by: bravebot_agent::skill_eval::a_withheld_vote_leaves_the_rubric_without_a_result`
`verified-by: bravebot_cli::skill_eval::a_rubric_passes_on_two_of_the_three_verdicts_the_judge_gives`
`verified-by: bravebot_cli::skill_eval::a_rubric_with_one_met_of_three_fails`
`verified-by: bravebot_cli::skill_eval::a_judge_is_never_shown_a_file_nobody_vouched_for`

<a id="SKILLEVAL-8"></a>
### SKILLEVAL-8: a run scores the weight of the graders that passed over the weight of all of them, and a case is the mean of its scored runs

A run with no weight to take a fraction of has no score. A case in an arm is the mean of the scores
of its runs that have one. The whole evaluation in an arm is the mean of the scores of its cases
that have one. The difference is the score with the skill minus the score without, and it exists
only where both arms have a score.

`verified-by: bravebot_agent::skill_eval::a_run_scores_the_fraction_of_its_graders_that_passed`
`verified-by: bravebot_agent::skill_eval::weights_change_what_a_grader_is_worth`
`verified-by: bravebot_agent::skill_eval::graders_with_no_weight_give_no_score`
`verified-by: bravebot_agent::skill_eval::a_case_is_the_mean_of_its_scored_runs`
`verified-by: bravebot_agent::skill_eval::the_difference_is_with_minus_without_and_needs_both_arms`

<a id="SKILLEVAL-9"></a>
### SKILLEVAL-9: the report prints `WITH`, `W/OUT` and the difference for each case and overall, and ends on a status that says what happened

Each case line shows the two scores and the difference, or says that an arm has no score. The
overall line follows. Runs stopped at their cap and refusals are counted in the report. The command
ends on the status for an argument (2) where it was refused, on the status for a configuration
error (3) where no model would serve a run, on the status for a limit (8) never, since a run
stopped at its cap is reported and does not end the evaluation, and on 0 when every run was made and
reported. Every sentence the command says is in the message catalog.

`verified-by: bravebot_cli::skill_eval::a_skills_cases_run_with_it_and_without_it_and_the_report_gives_both_scores`
`verified-by: bravebot_cli::skill_eval::an_arm_with_no_score_is_said_and_not_read_as_zero`

## Known costs

- **The baseline can still read the skill's file.** The arms differ by the skill's place in the
  catalogue, so a run without it that has `read_file` can open its `SKILL.md` by path and is then
  not entirely without the skill.

- **Variance is the number of repeats.** A case run three times per arm gives a mean over three
  runs of a model that varies, so a small difference between the arms is not a finding. Raising
  `repeats` costs a request per round per run.

- **A rubric is judged by the goal judge, with the same limits.** The verdict is the model's and
  reads only as MET, NOT MET, IMPOSSIBLE or unreadable. A judge that is wrong about a rubric is
  counted as it answered, and three verdicts reduce one wrong answer, not a consistently wrong
  judge. Each rubric costs three requests per run.
