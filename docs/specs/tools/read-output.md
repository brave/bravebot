---
id: OUTPUT
title: read_output
status: normative
governs:
  - crates/core/src/policy.rs
guards:
  - symbol: Policy::read_output
documented-by:
  - docs/website/docs/reference/tools.md
  - docs/website/docs/security/permissions.md
---

## Scope

Letting the planner read what a program printed. The reference naming the result is routing; there
are no content arguments. What a program's output is labelled in the first place is
[run.md](run.md).

## Clauses

<a id="OUTPUT-1"></a>
### OUTPUT-1: this is an assertion about bytes, and is not a relabel

A person is shown the bytes themselves with the command that printed them, and decides.

```
╭ let the model read this? ────────────────────────────────╮
│Read 1 line  printed by find /Applications -name 'Brave…' │
│                                                          │
│  the check found no attempt to give instructions in this │
│                                                          │
│  the model has not seen this. Approving puts it in its   │
│  context, and it will act on it.                         │
│                                                          │
│┃ /Applications/Brave Browser Nightly.app                 │
│                                                          │
│  y let it read this    n keep it back    ctrl-c stop     │
╰──────────────────────────────────────────────────────────╯
```

The slot keeps the label it was quarantined at. What the planner receives is a **new value** whose
first label comes from the provenance the policy layer tracked: a person having read it, or, where
somebody turned auto-vetting on, a check having found nothing ([CHECK-12](../vetting.md#CHECK-12)).
Which of the two it was is named in the trail. Either way it covers one result, needs a single-use
endorsement naming that slot, and the next run asks again.

Only output from `run` can be read this way. A file's worth is the trust map's answer, and a second
route to it would be a way to disagree with the first.

**Why.** This is the strongest assertion in the system and the only one made about bytes rather
than about a prediction: vouching guesses at output that does not exist yet, while this is a
statement about text in front of the reader. Errors count: a run that failed put its explanation on
stderr, and a planner that cannot see it will report that the command worked.

`verified-by: bravebot_core::policy::output_a_person_vouched_for_comes_back_trusted`
`verified-by: bravebot_core::policy::output_a_person_vouched_for_is_still_private`
`verified-by: bravebot_core::policy::vouching_for_output_does_not_relabel_the_slot`
`verified-by: bravebot_core::policy::an_approval_to_read_output_cannot_be_replayed`
`verified-by: bravebot_core::policy::output_released_by_a_safe_verdict_is_no_wider`
`verified-by: bravebot_core::policy::the_trail_says_which_of_the_two_released_the_output`
`verified-by: bravebot_agent::turn::output_a_person_reads_and_approves_reaches_the_planner`
`verified-by: bravebot_agent::turn::output_a_person_refuses_stays_out_of_the_planner`

<a id="OUTPUT-2"></a>
### OUTPUT-2: only output from a program can be read this way

A file's worth is the trust map's answer, and naming a file, opening a directory and the startup
question already give it. A second route to the same decision would be a way to disagree with the
first.

This rule is about a file's worth and not about a slot's bytes. Promoting one slot after a check
has read it is a different thing, single-use, writing no rule and leaving the trust map saying
what it said, and it is [vetting.md](../vetting.md).

`verified-by: bravebot_core::policy::a_file_cannot_be_promoted_by_reading_it_aloud`

<a id="OUTPUT-3"></a>
### OUTPUT-3: the person is told what a check made of the output before they answer

A confined check reads the slot before this prompt is drawn, and the word it gave and the sentence
it wrote are on the screen with the bytes. This is [CHECK-10](../vetting.md#CHECK-10) and the
reasoning is there; what is here is that a `read_output` prompt is one of the three it covers, and
that the check runs before the question rather than after the answer.

No expectation is sent with it. The planner asked for the output to be read, not for it to be
checked, and it has said nothing about what the command printed.

While auto-vetting is off, which is the default, the verdict decides nothing here: the word travels
to the prompt, the prompt draws it, and the answer is what releases the bytes. Where somebody turned
it on, a verdict of nothing found is what releases them, and every other verdict draws the prompt
where there is one to draw. [CHECK-12](../vetting.md#CHECK-12) is that rule and the reasoning for it.

A run bypassing permissions draws none, and there the verdict is the whole of the answer: nothing
found releases the output and every other verdict keeps it back, the refusal being what the screening
was asked for. Without the screening asked for the output is released unshown and no check is made
([MODE-4](../permission-modes.md#MODE-4)).

`verified-by: bravebot_agent::turn::an_output_offer_carries_what_a_check_said`
`verified-by: bravebot_tui::confirm::the_output_prompt_says_what_a_check_found`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_releases_command_output_unasked`
`verified-by: bravebot_agent::turn::with_auto_vetting_an_unsafe_verdict_still_asks_about_command_output`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_broken_check_still_asks_about_command_output`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_output_a_check_objected_to`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_output_no_check_could_be_made_about`
