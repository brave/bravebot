---
id: VET
title: vet_content
status: normative
governs:
  - crates/agent/src/tools.rs
documented-by: docs/website/docs/security/vetting.md
---

## Scope

Asking to be shown one quarantined slot after a confined check has read it. The reference naming
the slot is routing; `expects` is the planner's own sentence and is neither. What the check is,
what it may say, and what a person's answer does is [vetting.md](../vetting.md).

This is the one of the three checked prompts the planner asks for by name, and so the only one that
carries an `expects`. The other two come with a read the planner made for its own reasons:
[read-output.md](read-output.md) and the vouch offer in [trust-map.md](../trust-map.md#TRUST-8).
[CHECK-10](../vetting.md#CHECK-10) covers all three.

## Clauses

<a id="VET-1"></a>
### VET-1: the routing field is one slot, and it is the field a person approves

The call names one reference and says in a sentence what the planner expects it to hold. The
reference is the only field that decides anything, and the person is shown the bytes behind it,
so the field they approve is the field the call turns on.

`expects` is the planner's own words. It is sent to the check, so that a page can be judged
against what it was supposed to be, and it is drawn on the prompt, so that a person can see why
the planner wants this. It is not a destination and must not be private.

Where the content came from is said in the driver's words rather than in the planner's, and it is
said as a path, a URL or a command rather than as a reference name. A reference means something to
the planner and nothing at all to the person being asked about it.

`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`
`verified-by: bravebot_core::policy::a_private_expectation_cannot_direct_a_check`
`verified-by: bravebot_core::policy::a_check_says_where_the_content_came_from_and_not_which_slot_it_is_in`

<a id="VET-2"></a>
### VET-2: what it refuses

A reference to nothing, since there is nothing to check or to show. A picture, since a check reads
text and the bytes behind a picture slot are a data URI. A private sentence, per VET-1. And a call
from a delegate, which is neither offered the tool nor answered when it names it anyway.

A reference to a file nothing has read yet is opened rather than refused: naming one is the
ordinary way for the planner to ask about a file it may not read.

**Why a delegate is not offered it.** The prompt belongs to the person who set the sub-task going,
about content they never asked to see, in the middle of work they are not reading. What crosses
back from a delegate is [delegation.md](../delegation.md)'s question, and this would put bytes
into a context nobody at the keyboard is watching.

`verified-by: bravebot_core::policy::a_check_over_nothing_is_refused`
`verified-by: bravebot_core::policy::a_check_over_a_picture_is_refused`
`verified-by: bravebot_agent::tools::a_delegate_is_never_offered_a_way_to_promote_a_slot`

<a id="VET-3"></a>
### VET-3: the result is the bytes, or a refusal that says to work without them

Where the person agrees, the content comes back as text the planner may read. Where they do not,
the planner is told so and told to work with what it has or to say what it needed, rather than
being left to guess or to ask again. Nothing the check wrote goes back either way.

Where auto-vetting is on and the check found nothing, the content comes back with no prompt drawn
([CHECK-12](../vetting.md#CHECK-12)). The result the planner reads is the same in both cases: it is
told the bytes, and never which of the two answered or what the check said, so nothing it writes
can be aimed at one path rather than the other.

`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`
`verified-by: bravebot_agent::turn::content_a_person_refuses_after_a_check_stays_out_of_the_planner`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_reaches_the_planner_unasked`
