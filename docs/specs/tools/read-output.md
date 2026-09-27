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

Letting the planner read what a program printed. The reference naming the result is routing; the
only content argument is the `why` every tool takes ([TOOL-5](tool-surface.md#TOOL-5)). `offset` is
a literal, a byte count that names no destination. What a program's output is labelled in the first
place is [run.md](run.md).

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
first label comes from the provenance the policy layer tracked: a person having read it; or, where
somebody turned auto-vetting on, a check having found nothing ([CHECK-12](../vetting.md#CHECK-12));
or, in a run bypassing permissions with no screening asked for, the mode itself, with nobody shown
the bytes and no check made about them ([MODE-4](../permission-modes.md#MODE-4)). Which of the three
it was is named in the trail, and a release the mode made is never recorded as one of the other two:
an entry crediting a person who was never shown the bytes is the one a reader cannot check, and one
crediting a check names a call that was never placed. Whichever it was it covers one result, needs a
single-use endorsement naming that slot, and the next run asks again.

A release the mode makes may come in the result of the run itself, where that run asked with `read`
([RUN-22](run.md#RUN-22)). It is this release, through the same path: the same endorsement, the
same new value and the same trail entry, a round before a `read_output` call would have made it.

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
`verified-by: bravebot_core::policy::the_trail_says_which_of_the_three_released_the_output`
`verified-by: bravebot_agent::permission_mode::an_unscreened_unattended_release_is_credited_to_the_mode`
`verified-by: bravebot_agent::turn::an_unscreened_unattended_run_credits_the_mode_for_the_output`
`verified-by: bravebot_agent::turn::an_unscreened_unattended_run_that_asks_to_read_is_handed_its_output_in_the_same_result`
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

<a id="OUTPUT-4"></a>
### OUTPUT-4: output the planner may read, kept whole past a cap, is read a page at a time with nobody asked

Where what a run or a job printed was output the planner may read and too long for its result, the
planner was given the beginning and the end, and a reference to the whole
([RUN-21](run.md#RUN-21)). `read_output` on that reference hands back one page. It starts at the
byte `offset` names, or at zero where it names none, and is no longer than the cap the result was
cut to. A line after it says which bytes it holds and the offset the next page starts at, or that
it reached the end. Both ends land on a character boundary. Nobody is asked, no check is made and
nothing is endorsed. The page keeps the slot's label, and reaches the planner through the
presentation that decides from the label alone, as the whole would have without the cap.

What decides which of this and [OUTPUT-1](#OUTPUT-1) applies is the slot's label, never a byte it
holds. An `offset` on a slot the planner may not read is refused before anybody is asked or anything
is checked. The page the planner expects is not what `read_output` would hand it there, and a
question put to a person on its behalf would be one it did not mean to ask.

The reference to the whole says the planner may read it and how, not that it is quarantined. The
sample says where its middle begins, so the first page can start there.

**Why nobody is asked.** The cap is a budget and not an authority ([RUN-21](run.md#RUN-21)). The
label on these bytes already let the planner read them, and the only thing that kept them out of the
result was the room they would take. A person asked here would be answering for bytes the label had
already answered for, and the trail would credit them with a release that was never theirs.

**Why a page and not the whole.** The room the cap protects is the same whichever call spends it,
so a page is held to the figure the result was. The planner that meets a 198 KB log wants a part of
it, and paging hands over the part it names without running anything again.

**Why the reference stopped saying quarantined.** Told "you will not be shown it" about a 198 KB
`git log` whose beginning and end it had just read, a planner ran `git log` four more times through
`awk`, and each was cut and called quarantined again. None of it was out of the planner's reach.

`verified-by: bravebot_core::reference::kept_output_is_offered_page_by_page_and_not_called_quarantined`
`verified-by: bravebot_core::policy::content_the_planner_saw_a_sample_of_is_kept_whole`
`verified-by: bravebot_agent::tools::a_page_starts_and_stops_on_a_character_and_names_the_next`
`verified-by: bravebot_agent::turn::output_too_long_for_its_result_is_read_page_by_page_with_nobody_asked`
`verified-by: bravebot_agent::turn::an_offset_into_output_nobody_vouched_for_is_refused_and_nobody_is_asked`
`verified-by: bravebot_agent::turn::what_an_ended_job_printed_is_capped_with_the_whole_of_it_kept`
`verified-by: bravebot_agent::turn::output_a_person_reads_and_approves_reaches_the_planner`
