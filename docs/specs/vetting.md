---
id: CHECK
title: Vetting quarantined content
status: normative
governs:
  - crates/core/src/vetting.rs
  - crates/core/src/policy.rs
  - crates/agent/src/vet.rs
  - crates/agent/src/report.rs
  - crates/session/src/store.rs
  - crates/tui/src/status.rs
  - crates/ui-bridge/src/bridge.rs
  - crates/ui-bridge/src/settings.rs
  - crates/ui-bridge/src/wire.rs
  - crates/ui-bridge/src/turn.rs
  - ui/src/renderer/App.tsx
  - ui/src/renderer/components/Transcript.tsx
  - ui/scripts/ux-state.test.mjs
guards:
  - symbol: VettingSpec::new
  - symbol: Policy::before_vetting
  - symbol: Policy::before_vetting_a_path
  - symbol: Policy::compose_vetting_input
  - symbol: Policy::vetting_verdict
  - symbol: Policy::vetting_did_not_complete
  - symbol: Policy::promote_vetted
documented-by: docs/website/docs/security/vetting.md
---

## Scope

Reading quarantined content with a second model that holds nothing, so that the person deciding
whether the planner may have those bytes has a second opinion in front of them. What the check is
given, what it may say, which prompts run one, and what a person's answer to it does.

The tool the planner calls to ask for one slot is [tools/vet-content.md](tools/vet-content.md). The
prompts a check is drawn on are the ones [prompting.md](prompting.md) governs.

## Why it exists

Quarantined content has three ways out, and all three end at a person. What a program printed can
be read aloud, where a person reads the bytes and says the planner may have them
([tools/read-output.md](tools/read-output.md)). A file has the trust map, answered by naming the
path, by opening a directory, or at the startup question ([trust-map.md](trust-map.md)). One slot's
bytes can be promoted once, on the planner's request, and leave nothing behind
([tools/vet-content.md](tools/vet-content.md)). Everything else stays quarantined for the life of
the session.

Each of those is a person being asked to promote bytes they were not expecting and cannot audit at
speed, and the answer is the whole of the defence. A check is what puts a second opinion beside the
bytes before they answer, so the question is not the first time anything has looked at what the
content holds.

The single-use promotion is also there for the friction. Somebody who wants the agent to use a
fetched page otherwise has two moves: read every byte of it themselves, or vouch for a whole path,
which is very much larger. That third way covers one slot's bytes, once, and leaves nothing behind.

## Clauses

<a id="CHECK-1"></a>
### CHECK-1: a check holds less than a processor, and can write nothing at all

| | |
|---|---|
| Tools | none, and the request carries no tool list at all |
| Memory | none: the messages are built from nothing each time |
| Conversation | one request, one reply, no loop to steer |
| Reads | one piece of content, carried in the spec and fixed before the call |
| Writes | **nothing**: no slot, no reference, no destination of any kind |

A processor mints one slot or none. A check mints none, ever, and what it is fixed by holds no
field that could name a place for a result to go. The only things that come back are a word from
a fixed set and free text for a person to read.

The content is carried in the spec rather than named there, which is what "fixed before the call"
means: whatever the check will read is settled when the spec is built, and no store is left for a
second piece to be reached through. A check asked for by the planner takes it out of the slot it
names; a check before a vouch prompt takes it from the file, where there is no slot yet.

**Why.** The isolation is expressed as a type that cannot write rather than as a subprocess,
which is the same reasoning [processors.md](processors.md) gives for a processor: there is no
untrusted code here, so an operating-system boundary would confine the wrong thing. What is new
is that a check needs to be narrower still, because unlike a processor it produces something a
person is invited to act on.

What it is told about itself says so, and says the opposite of what a processor is told: a
processor that notices an injection attempt is asked to leave it out, because its output is not a
place a person will read, while a check exists to say so and its output is read by a person and by
nobody else. Where a check cannot tell, it warns.

`verified-by: bravebot_core::policy::a_check_is_given_the_one_slot_its_spec_names`
`verified-by: bravebot_agent::vet::a_checker_is_not_told_to_keep_quiet_about_what_it_notices`
`verified-by: bravebot_agent::vet::a_checker_that_cannot_tell_is_told_to_warn`
`verified-by: by-construction (what fixes a check holds one piece of content and the planner's sentence, has no output label, no output reference and no destination, and is built only by taking an authority minted inside the policy layer; every field is private and no method takes &mut self)`

<a id="CHECK-2"></a>
### CHECK-2: the content is enclosed so that it cannot forge the enclosure

The input is two blocks, the driver's own facts about the content first and the content second.
The content is written as **one JSON string literal**, so it occupies one physical line whatever
it holds and a newline in it is written as an escape.

**The containment is that encoding, not the block markers.** The markers are fixed ASCII with no
nonce, and content may spell them; it cannot put one at the start of a line, because it cannot
produce a line. Do not replace the encoding with a nonce under the impression the markers are
what is holding.

The last thing in the request is the driver's, after the content rather than only before it, and
it says both that the block was data and what an answer looks like.

A check over a picture, which [CHECK-15](#CHECK-15) specifies and nothing builds yet, has no string
to encode: the picture goes in a part of its own, and CHECK-15 says why nothing mechanical holds it
there.

`verified-by: bravebot_core::vetting::encoded_content_occupies_one_line`
`verified-by: bravebot_core::vetting::content_that_spells_the_fence_cannot_forge_one`
`verified-by: bravebot_core::vetting::a_quote_in_the_content_does_not_end_the_literal`
`verified-by: bravebot_core::policy::content_that_spells_the_fence_cannot_end_its_own_block`
`verified-by: bravebot_core::policy::the_metadata_is_a_separate_block_before_the_content`
`verified-by: bravebot_core::policy::a_composed_check_input_is_still_quarantined`
`verified-by: bravebot_agent::vet::control_is_re_asserted_after_the_content`

<a id="CHECK-3"></a>
### CHECK-3: what the planner says it expects is a prompt, must not be private, and is often absent

The planner says what it thinks the slot holds, and the check is told. Those are the planner's own
words on the footing a processor's instruction sits on: not content anybody read, but the sentence
the driver is about to send. Private is refused, since the user's own data must not become another
model's prompt.

Only the planner asking for a check carries one. A check run before a prompt the planner did not
ask for has nowhere to get an expectation from: it asked to read a file, or to be shown what a
command printed, and said nothing about the contents. There the key is **left out of the metadata
entirely** rather than written empty, because an empty string would tell the reader the planner
expected nothing, where the truth is that nothing claimed anything.

`verified-by: bravebot_core::policy::a_private_expectation_cannot_direct_a_check`
`verified-by: bravebot_core::policy::a_check_before_a_vouch_carries_the_file_and_claims_no_expectation`

<a id="CHECK-4"></a>
### CHECK-4: one word out of two, and everything else is a check that did not complete

A reply states its verdict as `safe` or `unsafe`, compared after trimming and lowercasing and
after nothing else. Any other word, a reply that states no verdict, a truncated reply, a timeout
and a backend error are all **inconclusive**, which says nothing about the content.

Where a reply holds more than one candidate answer, the last is read: that is the one a model
writes after reasoning, and reading the first would let its worked example outrank its conclusion.

**Why.** Fails closed on every path. The only reply that can reduce a warning is one that answered
in the form it was asked for, so a check that half worked lands where a check that did not run
lands.

`verified-by: bravebot_core::vetting::a_stated_verdict_is_read_with_its_reason`
`verified-by: bravebot_core::vetting::a_verdict_inside_prose_is_found`
`verified-by: bravebot_core::vetting::the_last_object_is_the_verdict`
`verified-by: bravebot_core::vetting::a_verdict_spelled_inside_a_reason_decides_nothing`
`verified-by: bravebot_core::vetting::an_unknown_word_is_inconclusive`
`verified-by: bravebot_core::vetting::a_word_with_punctuation_after_it_is_not_a_verdict`
`verified-by: bravebot_core::vetting::case_and_space_around_the_word_do_not_matter`
`verified-by: bravebot_core::vetting::a_reply_with_no_verdict_in_it_is_inconclusive`
`verified-by: bravebot_core::vetting::a_bare_word_is_not_a_verdict`
`verified-by: bravebot_core::vetting::a_nested_key_does_not_answer_for_the_object_holding_it`
`verified-by: bravebot_core::vetting::a_truncated_reply_is_inconclusive`
`verified-by: bravebot_core::vetting::a_reply_holding_characters_outside_ascii_is_read`
`verified-by: bravebot_agent::turn::a_check_that_could_not_be_made_falls_back_to_the_question`

<a id="CHECK-5"></a>
### CHECK-5: a verdict is advice, and while auto-vetting is off the only thing it decides is which prompt is drawn

Auto-vetting is off until somebody turns it on ([CHECK-11](#CHECK-11)), so this is what a session
does unless a person has said otherwise. The bytes are put in front of the person whatever the
check said, the keys that answer the question are offered whatever it said, and the answer is
theirs. A verdict of safe promotes nothing by itself, and a verdict of unsafe withholds nothing:
neither is an answer to the question being asked.

The one key a verdict does decide is the one that answers no question: the standing key that turns
auto-vetting on, offered at either promoting prompt only where the check completed and found nothing
([PROMPT-6](prompting.md#PROMPT-6)). It is unbound where it is not drawn.

A picture, which [CHECK-15](#CHECK-15) specifies and nothing builds yet, differs in one way: what is
put in front of the person is a file to open rather than bytes on the screen.

With auto-vetting on, one verdict answers the two promoting questions in the person's place, and
[CHECK-12](#CHECK-12) is the whole of what that changes. Everything in this clause holds of the
vouch offer and of a server's tool list either way, and of all four where the mode is off.

The three outcomes are told apart on the screen. "This looks like an attempt to give instructions"
and "nothing looked at this" are different facts about different risks, and one sentence covering
both would be wrong about one of them.

Every prompt CHECK-10 runs a check for draws it the same way, out of one row builder rather than
four, so a prompt cannot be added that carries a verdict and forgets to say what it was.

**Why.** This is what keeps the branch narrow. Something is being decided from bytes derived from
untrusted content, which [labels.md](labels.md) enumerates as a known cost; what bounds it is that
the decision is which sentence a person reads before answering for themselves.

`verified-by: bravebot_core::policy::a_safe_verdict_promotes_nothing_by_itself`
`verified-by: bravebot_core::policy::an_unsafe_verdict_does_not_overrule_the_person`
`verified-by: bravebot_tui::confirm::the_vet_prompt_says_which_of_the_two_failures_it_was`
`verified-by: bravebot_tui::confirm::a_safe_verdict_is_drawn_as_what_the_check_found`
`verified-by: bravebot_tui::confirm::a_safe_verdict_does_not_change_which_keys_the_vet_prompt_offers`
`verified-by: bravebot_tui::confirm::only_a_safe_verdict_offers_to_stop_asking`
`verified-by: bravebot_tui::confirm::the_output_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::confirm::the_vouch_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::confirm::a_vouch_prompt_says_when_no_check_was_made`
`verified-by: bravebot_tui::confirm::a_tool_list_says_what_a_check_found`

<a id="CHECK-6"></a>
### CHECK-6: promoting is an assertion about one slot's bytes, and writes no rule

The slot keeps the label it was quarantined at. What the planner receives is a **new value** whose
first label comes from the provenance the policy layer tracked, which is a person having read the
bytes and said so.

Nothing reaches the trust map. A later read of the same file mints a new slot and is quarantined
exactly as it is today, and the same content asked about again asks again.

**Why.** That is what keeps this from being a second answer to what a file is worth.
[tools/read-output.md](tools/read-output.md) refuses a file outright on the grounds that a second
route to a path's worth would be a way to disagree with the trust map. This is not that route: the
trust map still says the path is untrusted, and reading it back still quarantines it.

`verified-by: bravebot_core::policy::vetted_content_a_person_vouched_for_comes_back_trusted`
`verified-by: bravebot_core::policy::promoting_a_vetted_slot_does_not_relabel_it`
`verified-by: bravebot_core::policy::vetting_a_slot_vouches_for_no_path`
`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`

<a id="CHECK-7"></a>
### CHECK-7: confidentiality is never lowered, so vetting unlocks no egress

What comes back is private. The bytes may have come out of the workspace, and nothing about a
check or about a person reading them makes them fit to leave the machine or to become a routing
field.

`verified-by: bravebot_core::policy::a_private_slot_promotes_to_private_and_never_to_public`

<a id="CHECK-8"></a>
### CHECK-8: an approval covers one slot once, and answers no other question

A single-use endorsement naming that exact slot is what authorises the promotion. It cannot be
replayed, and an approval given to a different question is not one of these: an approval to read
what a program printed promotes nothing.

Three things mint one and nothing else does: a person answering the prompt; where auto-vetting is
on, a safe verdict on either route [CHECK-12](#CHECK-12) names; and a run bypassing permissions with
no screening asked for, where the mode answers both prompts without drawing either
([MODE-4](permission-modes.md#MODE-4)). Which of the three it was is recorded, on both routes,
because a trail that credited a person who was never shown the bytes would be the one record a
reader cannot check, and one that credited a check nobody made would name a call that was never
placed. Everything else about the endorsement is the same whichever it was: one slot, once, and no
other question answered.

For a picture, which [CHECK-15](#CHECK-15) specifies and nothing builds yet, only the first two
mint one: a run bypassing permissions with no screening asked for refuses a picture rather than
answering for it ([tools/vet-content.md](tools/vet-content.md#VET-4)).

`verified-by: bravebot_core::policy::content_cannot_be_promoted_without_an_endorsement`
`verified-by: bravebot_core::policy::an_approval_to_vet_cannot_be_replayed`
`verified-by: bravebot_core::policy::the_trail_says_when_nobody_was_asked`
`verified-by: bravebot_core::policy::the_trail_says_when_the_mode_promoted_a_slot_unshown`
`verified-by: bravebot_core::policy::the_trail_says_which_of_the_three_released_the_output`
`verified-by: bravebot_core::vetting::every_endorsement_is_described_differently`
`verified-by: bravebot_agent::turn::an_unscreened_unattended_run_credits_the_mode_for_a_promoted_slot`
`verified-by: bravebot_core::policy::a_promotion_nobody_was_asked_about_is_no_wider`
`verified-by: bravebot_core::policy::an_approval_to_read_output_is_not_an_approval_to_vet`
`verified-by: bravebot_tui::remote_confirm::an_approved_output_read_does_not_approve_a_vetted_read`
`verified-by: bravebot_tui::remote_confirm::a_closed_channel_refuses_a_vetted_read`

<a id="CHECK-9"></a>
### CHECK-9: what the check says reaches a person, and no model

The verdict word and the sentence the check wrote go to the prompt. Neither reaches the planner,
whichever way the person answers, and neither reaches a second check.

The sentence is free text about content an attacker may own, so it is untrusted and private like
the content, and it is drawn inside the margin a prompt puts round anything nobody vouched for.
The audit trail records the word and never the sentence: a trail is read by people entitled to
assume the driver is talking. Where the word is `inconclusive` the driver's own account of why is
recorded with it, that account being the driver's sentence and not the check's, so a refusal on a
check that objected and a refusal on a check that could not be read are told apart afterwards by
somebody who was never asked either question.

`verified-by: bravebot_core::policy::what_a_check_says_is_as_untrusted_as_what_it_read`
`verified-by: bravebot_core::policy::the_trail_records_the_verdict_and_never_the_reason`
`verified-by: bravebot_core::policy::the_trail_tells_an_objection_apart_from_a_check_that_said_nothing`
`verified-by: bravebot_core::policy::a_check_that_could_not_be_made_records_the_word_and_the_drivers_own_account`
`verified-by: bravebot_agent::turn::the_trail_records_the_verdict_of_a_check_that_could_not_be_made`
`verified-by: bravebot_agent::turn::content_a_person_refuses_after_a_check_stays_out_of_the_planner`
`verified-by: bravebot_agent::turn::a_vouch_offer_carries_what_a_check_said_about_the_whole_file`
`verified-by: bravebot_agent::turn::an_output_offer_carries_what_a_check_said`
`verified-by: bravebot_tui::confirm::what_the_check_said_is_drawn_inside_the_margin_too`

<a id="CHECK-10"></a>
### CHECK-10: every prompt that would promote quarantined content runs a check first

All four ways out of quarantine end at a person answering a prompt, and a check runs before the
prompt is drawn, not after it is answered:

| The prompt | What a yes does | What the check reads |
|---|---|---|
| [tools/vet-content.md](tools/vet-content.md) | promotes one slot's bytes once | the slot the planner named |
| [tools/read-output.md](tools/read-output.md) | releases what a program printed to the planner | the slot the planner named |
| the vouch offer in [tools/read-file.md](tools/read-file.md) | writes a rule about the path | **the whole file**, not the preview |
| a server's tool list in [mcp-servers.md](mcp-servers.md#SERVERS-8) | offers the list to the planner, and records its digest | **the whole list**, as it is drawn |

**Why the whole file.** A vouch prompt shows the head of the file and a yes grants all of it, so a
check over the preview would report on the part an injection attempt has the least reason to be in.
The check reads what the answer covers.

**Why all four.** Otherwise the quickest way past a check is to ask for the prompt that does not
run one. A person asked to promote content is asked on the strength of what they can see; some of
these prompts having a second opinion on them and another not is a gap the planner chooses, and
the trust map's rule is the largest grant of them. The planner does not ask for the list prompt,
but a server wrote every word on it, and a yes puts those words in front of the planner for every
turn after.

The one exemption is [permission-modes.md](permission-modes.md#MODE-4)'s bypassing mode, which draws
no prompt at all. A check is still made for the two promotion prompts where that run asked for
auto-vetting, because there the word answers in the absent person's place and an unscreened promotion
is what the screening was asked for to stop. It is made nowhere else in that mode: the vouch offer
and a server's tool list read no word there whatever was asked for, and a promotion in a run that
asked for no screening
reads none either, so a call would produce a word nobody reads. Where none is made the verdict filled
in is `inconclusive`, which claims nothing.

`verified-by: bravebot_agent::turn::a_vouch_offer_carries_what_a_check_said_about_the_whole_file`
`verified-by: bravebot_agent::turn::an_output_offer_carries_what_a_check_said`
`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`
`verified-by: bravebot_agent::turn::bypassing_makes_no_check_before_promoting_content`
`verified-by: bravebot_agent::turn::bypassing_fills_in_a_verdict_that_claims_nothing`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_content_a_check_objected_to`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_output_a_check_objected_to`
`verified-by: bravebot_core::policy::a_check_before_a_vouch_carries_the_file_and_claims_no_expectation`
`verified-by: bravebot_core::policy::a_check_before_a_tool_list_reads_the_list_it_is_about`

<a id="CHECK-11"></a>
### CHECK-11: auto-vetting is off until somebody turns it on, and there are three ways in

Nothing about a check answers a question until a person has said it may. Three routes say so, and
they differ in how long the answer lasts rather than in what it says:

| Route | Read from | Lasts |
|---|---|---|
| `--vet` | the command line ([cli.md](cli.md#CLI-15)) | this run |
| `~/.bravebot/vetting`, one word, written by the standing key at a vetting prompt | the person's own directory | until they change it |
| `"vetting": { "auto": true }` | `~/.bravebot/settings.json`, the **home layer only** | every session |

Any one of them is enough. A choice recorded in the person's own directory outranks the settings
key, which is the precedence the editing style already uses: somebody who turned the mode off has
made a decision that has to outlast the session, so off is written to that file rather than the
file being removed, and a settings file cannot turn it back on for them tomorrow. `--vet` outranks
both, a recorded `off` included, because it is the narrowest in time: somebody typing it has said
what they want of the run in front of them, and that is the footing
[permission-modes.md](permission-modes.md)'s own flag sits on, which is a strictly larger thing
anything able to pass `--vet` could pass instead. There is no flag the other way
([CLI-15](cli.md#CLI-15)).

The key that writes that file is drawn on the vetting prompt, at the moment the mode would have
saved the person a keystroke, which is where [trust-map.md](trust-map.md#TRUST-8) offers its own
larger grant. It is offered only where the check found nothing, and
[PROMPT-6](prompting.md#PROMPT-6) is why. Nothing writes the settings file: no shipping code here
edits a person's `settings.json`, and doing it would mean a JSON rewrite that preserves their
comments and key order.

The recorded choice is read only by a session that records one. A session asked to leave nothing
behind reads the model and the theme, because those decide what it looks like; this decides whether
somebody is asked, and a private session inheriting that answer is the one read worth refusing.
[incognito.md](incognito.md) names it as the one read that mode refuses, so the exception is stated
where somebody reading about that mode finds it and not only here. The other two routes are
unaffected there: a flag names what somebody wants of the run in front of them, and the settings key
is not a decision a session recorded.

**A session that opened with the mode on says so, and goes on saying so.** It is said once at the
top of the transcript, before the first slot can reach it, and reported in `/status` for the rest
of the session with the file it is kept in named. A note scrolls away, and the one thing a person
cannot read off a transcript is a question that was never put: a standing answer that stops a
prompt appearing has to be readable at the moment they wonder why. That is the rule
[permission-modes.md](permission-modes.md)'s own modes follow, and a session that is asking says
nothing, since a line reporting the ordinary state is a line people learn to skim.

**The settings key is read from the home layer and no other.** A project `.bravebot/settings.json`
naming it, a machine-local one, and a file the command line named are each reported by `doctor` and
not obeyed. Every other name in those files configures where a request goes or how the interface
behaves; this one says whether a person is asked before content nobody vouched for reaches the
planner, so a line in a checkout could turn the asking off for whoever opened it. A word the file
does not know, and a value that is not a boolean, are no answer at all rather than a guess.

`verified-by: bravebot_core::vetting::nothing_is_auto_vetted_until_somebody_asks_for_it`
`verified-by: bravebot_core::vetting::each_of_the_three_routes_turns_it_on_by_itself`
`verified-by: bravebot_core::vetting::a_recorded_choice_outranks_the_settings_key`
`verified-by: bravebot_core::vetting::the_flag_outranks_a_recorded_choice`
`verified-by: bravebot_config::settings::the_home_layer_may_ask_for_auto_vetting`
`verified-by: bravebot_config::settings::a_project_layer_cannot_turn_auto_vetting_on`
`verified-by: bravebot_config::settings::the_local_layer_cannot_turn_auto_vetting_on_either`
`verified-by: bravebot_config::settings::a_named_layer_cannot_turn_auto_vetting_on`
`verified-by: bravebot_config::settings::a_project_layer_does_not_override_what_the_home_layer_said_about_vetting`
`verified-by: bravebot_config::settings::a_vetting_key_that_is_not_a_boolean_says_nothing`
`verified-by: bravebot_session::store::a_recorded_answer_about_vetting_is_read_back_both_ways`
`verified-by: bravebot_tui::persist::a_recorded_answer_about_vetting_outlives_the_session_that_gave_it`
`verified-by: bravebot_tui::incognito::the_standing_answer_about_vetting_is_not_read`
`verified-by: bravebot_session::store::a_file_naming_no_answer_about_vetting_is_not_a_choice`
`verified-by: bravebot_tui::state::a_session_asks_until_something_says_otherwise`
`verified-by: bravebot_tui::state::the_flag_and_the_settings_key_each_reach_the_session`
`verified-by: bravebot_tui::state::pressing_the_standing_key_turns_vetting_on_for_the_session`
`verified-by: bravebot_tui::confirm::only_a_safe_verdict_offers_to_stop_asking`
`verified-by: bravebot_tui::status::a_session_that_stopped_asking_about_a_check_says_so`
`verified-by: bravebot_tui::status::an_ordinary_session_says_nothing_about_a_check`
`verified-by: bravebot_config::settings::a_layer_that_named_only_vetting_is_not_a_layer_that_said_nothing`
`verified-by: bravebot_cli::main::the_vet_flag_is_taken_out_wherever_it_appears`
`verified-by: bravebot_ui_bridge::settings::auto_vetting_is_off_until_somebody_turns_it_on`
`verified-by: bravebot_ui_bridge::settings::the_home_settings_file_turns_auto_vetting_on`
`verified-by: bravebot_ui_bridge::settings::a_checkout_cannot_turn_auto_vetting_on`
`verified-by: bravebot_ui_bridge::settings::a_recorded_choice_outranks_the_home_settings_file`
`verified-by: bravebot_ui_bridge::vetting::a_session_says_auto_vetting_is_off_when_nobody_turned_it_on`
`verified-by: bravebot_ui_bridge::vetting::a_checkout_cannot_have_a_session_open_with_auto_vetting_on`
`verified-by: bravebot_ui_bridge::vetting::auto_vetting_is_reported_when_a_session_opens_and_held_for_the_rest_of_it`

<a id="CHECK-12"></a>
### CHECK-12: with it on, a safe verdict promotes one slot, and nothing else does

Where auto-vetting is on, a check that completed and found nothing answers in the person's place at
either prompt that promotes one slot's bytes: no prompt is drawn and the bytes reach the planner.
Every other verdict falls back to that prompt, carrying the banner it would have carried anyway, and
[CHECK-5](#CHECK-5) governs it from there. Unsafe and a check that did not complete are still told
apart on the screen, because the reason for asking is different in the two cases.

A picture, which [CHECK-15](#CHECK-15) specifies and nothing builds yet, is answered the same way,
on the first of the routes in the table below.

**Where there is nobody to fall back to, the fallback is a refusal.** A run bypassing permissions
([MODE-4](permission-modes.md#MODE-4)) puts no prompt to anybody, so a verdict that is not `safe` has
nothing to hand the question to. The bytes are kept back rather than promoted with a warning drawn on
a screen nobody is reading, and the planner is told the slot was kept from it and nothing more. The
same two prompts and no others: a vouch offer is not one of them here either.

**It covers the promotions and not the rule.** The four prompts a check runs for divide two to two:

| The prompt | What a yes does | With the mode on |
|---|---|---|
| [tools/vet-content.md](tools/vet-content.md) | promotes one slot's bytes once | a safe verdict answers |
| [tools/read-output.md](tools/read-output.md) | promotes one slot's bytes once | a safe verdict answers |
| the vouch offer in [tools/read-file.md](tools/read-file.md) | writes a rule about the path | still asks |
| a server's tool list in [mcp-servers.md](mcp-servers.md#SERVERS-8) | offers the list for the session, and records it | still asks |

**Why the line falls there.** It falls on the shape of the grant, not on which tool produced the
bytes. Both promotions cover one slot's bytes once and leave nothing behind ([CHECK-6](#CHECK-6)),
so what a verdict can buy is bounded by a single slot either way: the same question, about the same
kind of content, answered by the same check, ending in the same `(T,priv)` value and the same
single-use endorsement. A person offered the standing answer at one of them and not the other would
be reading which tool the planner happened to call, which is not a fact about the risk they are
being asked to take. A trust rule is different in kind, being a standing decision about a whole path
rather than about bytes in front of a reader, and the mode does not touch it. Nor does it touch a
server's tool list, which is read by the planner on every turn of the session once it is offered.
Widening the mode to either would be letting a check answer a bigger question than the one it read.

**What a person reads instead of being asked.** The route the mode covers most often in practice is
the output prompt: a run's output is quarantined by default, so it is the prompt a person meets when
they ask what a command printed. That is the reason the mode reaches it, and equally the reason the
mode is off until somebody turns it on.

What the mode never decides is anything but who answers. The slot is the planner's choice either
way, the label is `(T,priv)` either way ([CHECK-7](#CHECK-7)), the endorsement is single-use either
way, and no trust rule is written either way. [labels.md](labels.md) enumerates what an attacker who
owns the content gains from this, which is the reason it is off by default.

`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_reaches_the_planner_unasked`
`verified-by: bravebot_agent::turn::with_auto_vetting_an_unsafe_verdict_still_asks`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_check_that_could_not_be_made_still_asks`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_releases_command_output_unasked`
`verified-by: bravebot_agent::turn::with_auto_vetting_an_unsafe_verdict_still_asks_about_command_output`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_broken_check_still_asks_about_command_output`
`verified-by: bravebot_agent::turn::auto_vetting_does_not_answer_the_vouch_offer`
`verified-by: bravebot_ui_bridge::vetting::with_auto_vetting_on_a_safe_verdict_reaches_the_planner_with_no_prompt_put`
`verified-by: bravebot_ui_bridge::vetting::with_auto_vetting_off_a_safe_verdict_is_still_put_to_the_window`
`verified-by: bravebot_agent::permission_mode::screening_under_bypass_refuses_what_a_check_would_not_pass`
`verified-by: bravebot_agent::permission_mode::screening_under_bypass_still_promotes_what_a_check_found_nothing_in`
`verified-by: bravebot_agent::permission_mode::screening_does_not_reach_the_vouch_offer`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_content_a_check_objected_to`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_promotes_content_a_check_found_nothing_in`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_output_a_check_objected_to`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_output_no_check_could_be_made_about`
`verified-by: bravebot_core::policy::a_promotion_nobody_was_asked_about_is_no_wider`
`verified-by: bravebot_core::policy::output_released_by_a_safe_verdict_is_no_wider`
`verified-by: bravebot_core::policy::the_trail_says_which_of_the_three_released_the_output`

<a id="CHECK-13"></a>
### CHECK-13: every surface a promotion prompt reaches is given the verdict, and none of them is given an answer

Wherever one of those prompts is put to a person on a surface in another process, what the check
said travels with the question: the verdict word, and the sentence where there is one. It travels as
the verdict rather than as a sentence composed for a screen, so the surface renders its own words
and two different verdicts cannot arrive looking alike. A verdict that could not be reached is
carried as such and not as an absence, since a check that did not complete is a different thing to
be told from a check that found nothing.

Nothing about the verdict answers the question on that surface. Approving is a reply the person
makes, and a reply is held to the question it answers: it is single-use, it is matched to the
question that was asked, and a reply of another kind does not answer this one. A surface that
produced an approval from a verdict would be deciding on the strength of a word an attacker steers.

**Why.** [CHECK-9](#CHECK-9) is that what the check says reaches a person and no model, and a second
surface is where both halves could quietly fail: dropping the verdict leaves somebody answering the
question that most needs a second opinion with nothing but the bytes, which is the case that made a
check exist at all, and sending a sentence instead of the verdict makes the distinctions
[CHECK-4](#CHECK-4) draws a matter of wording.

The other half is why this is stated rather than left to [CHECK-8](#CHECK-8). Out here the reply
arrives as data from another process, so "an approval covers one slot once" is a property of what
this side accepts rather than of a key somebody pressed. The verdict is on the same wire as the
question, so a boundary that let one stand in for the other would be reading an attacker's word as a
person's answer.

`verified-by: bravebot_ui_bridge::wire::approval_evidence_is_kept_beside_the_decision`
`verified-by: bravebot_ui_bridge::refusal::vetted_content_never_approves_itself_or_consumes_another_kind_of_reply`
`verified-by: bravebot_ui_bridge::refusal::vetted_content_requires_its_own_explicit_approval`
`verified-by: bravebot_ui_bridge::refusal::an_approval_cannot_be_replayed`


<a id="CHECK-14"></a>
### CHECK-14: a check says that it is running, and says when it is over

A check is announced as it begins, with how many lines it was given, and announced as over however
it ended. Neither half carries anything else: not a fragment of the content, not the verdict, not
the sentence. The count is the shape of what was sent rather than any part of what it holds, and it
is the one figure that predicts how long the wait will be. A check over a picture, which
[CHECK-15](#CHECK-15) specifies and nothing builds yet, is announced as a picture or as a PDF
instead.

The end is announced on every way out, the failure that becomes a verdict nobody could read
included. A display left saying a check is running because the backend was down is the state this
pair exists to remove, and it is exactly the state a surface that only ever hears the beginning
ends in.

The interactive terminal and the desktop window both say it, and each says it ahead of the phase
rather than as one: the round's phase does not change while a check runs, so it is the one word on
the screen that is not what the session is waiting on, and it is what the surface goes back to
saying once the check is over. A surface that is handed the pair and draws neither half leaves the
finished-looking row that made the pair necessary.

**Why.** A whole model call runs inside a tool call, and the verb already on the screen names the
thing that has not happened yet: somebody watching `Read output` cannot tell a check that is working
from a backend that is hanging. Where auto-vetting is on and the verdict is safe, no prompt is drawn
for it either ([CHECK-12](#CHECK-12)), so without this the wait is silent from end to end.

**This is not what [CHECK-9](#CHECK-9) keeps back.** Saying that a check is running is not showing
what it says. What reaches a person here is that one is in flight and how much it was given; the
verdict reaches them on the prompt it is drawn on and reaches no model at all. Nor does any of it
ask them anything: progress announces, and a listener that has gone away is not an error.

`verified-by: bravebot_agent::turn::a_check_says_how_many_lines_it_is_reading_and_then_that_it_is_over`
`verified-by: bravebot_agent::turn::a_check_whose_call_fails_still_says_it_is_over`
`verified-by: bravebot_tui::state::a_running_check_names_the_indicator_ahead_of_the_phase`
`verified-by: bravebot_tui::state::a_check_that_is_over_gives_the_word_back_to_the_phase`
`verified-by: bravebot_tui::state::a_finished_turn_leaves_no_check_running`
`verified-by: bravebot_ui_bridge::reporting::a_check_crosses_as_a_pair_carrying_only_its_size`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so it is pinned instead by ui/scripts/ux-state.test.mjs, which folds the pair through the window's own reducer and reads the word both places draw it from, asserting that a running check takes the word from every phase, that a count of one reads as one line and a count of zero is still a check, that the word goes back to the phase once the check is over, and that a check whose end was never heard does not outlive a consolidating or failed turn; make check-ui and the Front end CI job both run it, and the governs list above holds the file to existing)`

<a id="CHECK-15"></a>
### CHECK-15: a check over a picture or a PDF is given the file itself, and its verdict counts as one about text does

Nothing builds this yet. It is the check half of the picture route
[tools/vet-content.md](tools/vet-content.md#VET-4) specifies, and until that is built no check is
made over a picture, because [VET-2](tools/vet-content.md#VET-2) refuses one before a check runs. A
PDF is a picture here, as [READ-5](tools/read-file.md#READ-5) reads one, and this clause says where
it goes differently.

The check is given the picture in a part of its own, as a processor is given one, after the driver's
block of facts about it and before the driver's words that close the request. A PDF goes in the part
a processor's PDF goes in, which Bedrock carries as a document rather than as a picture. It is never
given base64 text, which a model reads as characters and not as what the file shows. What it is told
about itself says the content is a file in a part of its own rather than a string inside a block.

**[CHECK-2](#CHECK-2)'s encoding has no counterpart here.** Text is contained because it cannot
produce a line. A picture puts no text into the request at all, and the boundary between its part
and the driver's is the whole of the enclosure. Words drawn in a picture can say anything the
driver's own words say, and whether a model tells the one from the other is the model's to get
right. A PDF is the sharper case: a backend may hand its model the document's text as text, which is
then words with nothing around them at all. Nothing mechanical holds either.

**Its verdict counts as one about text does.** With auto-vetting off it decides which sentence the
prompt draws and whether the standing key is offered ([CHECK-5](#CHECK-5)). With it on, a safe
verdict promotes the picture unasked and any other verdict asks ([CHECK-12](#CHECK-12)). Where
nobody can be asked, it answers only where somebody said in advance that it may, as it does for
text. A person asked about a picture reads the check's sentence before anything else, and one who
does not open the file answers on that sentence alone, so a verdict kept to deciding the sentence
would still decide most of what they know. What bounds a safe verdict is what bounds one about
text: one slot, once, `(T,priv)` ([CHECK-7](#CHECK-7)), and no trust rule.

**Why a check at all.** A person looking at a screenshot misses words a model reads: text too faint
or too small to notice at the size it opens at. The check is a model, so it reads what the planner
would read, and its sentence is where a person is told that a picture holds words addressed to
whoever reads it next. For a PDF it can read more than the person is shown: where a backend hands
its model a text layer the pages do not draw, the check reads that too. That is the second opinion
[CHECK-10](#CHECK-10) puts in front of every promoting prompt.

A backend that refuses the picture fails the request, and a failed request is inconclusive
([CHECK-4](#CHECK-4)), so the prompt says nothing looked at the picture. The check is announced as a
picture, or as a PDF, rather than with a count of lines ([CHECK-14](#CHECK-14)): a data URI is one
line, so the count would describe nothing.

**What it costs.**

- **Words in a picture can argue with the check unenclosed.** A picture drawn to say it is safe
  reaches the checker as something it reads, with no encoding between the two, so it has a better
  chance of being called `safe` than the same words as text. Where a safe verdict answers alone,
  with auto-vetting on or a screened run nobody is watching, that is the whole of what stands
  between the picture and the planner. It is the attack [labels.md](labels.md) lists as forcing the
  word `safe`, made easier.
- **A backend that drops the picture without saying so returns a verdict about the driver's words
  alone.** Nothing on this side can tell that from a verdict about the picture, so the prompt says
  what a check found in a picture no check looked at, and where a safe verdict answers alone the
  picture is promoted on it. A model the roster lists as not taking the file is refused before the
  check ([tools/vet-content.md](tools/vet-content.md#VET-4)); one thought to take it whose backend
  drops it anyway is what is left.
- **What a check reads of a PDF is its backend's choice.** One backend hands its model the text
  layer, another images of the pages, another both, and the verdict is about whatever was handed
  over. The person is told what the check found and not which of those it looked at.

What the rest of the route costs is in [tools/vet-content.md](tools/vet-content.md#VET-4).

`verified-by: none`

## Known costs

- **A quarantined slot's contents reach the backend, a second time.** A check is a model call, so
  asking about a page sends it where the page would only have gone if a processor had been asked
  about it. The destination is the one every other call already goes to, and the reader holds
  nothing, but the call happens whether or not the person then approves.

- **A read of a quarantined file now sends the file, before anybody has agreed to anything.**
  CHECK-10 puts a check in front of the vouch prompt, and the check is a model call over the whole
  file. So a planner that reads a path nobody vouched for causes those bytes to reach the backend
  in the confined conversation even where the person then says no, and the refusal keeps the bytes
  out of the planner rather than off the wire. That is a real widening of what a `read_file` on a
  quarantined path does, and it is the price of the person being told something before they answer.
  It is bounded by the offer being made once per path per turn, by never being made for a picture
  or a directory, and by the confined conversation holding no tools, no memory and no destination.

- **The check is a model, so it is wrong sometimes, in both directions.** A safe verdict on
  content that is an attack draws the quiet banner under bytes the person still reads; an unsafe
  verdict on an ordinary page draws the warning under bytes that are fine. Neither changes what
  approving does, and the second is the direction the prompt is built to fail in.

- **A plausible sentence may persuade somebody to skim.** The check's own words are attacker
  reachable and can lie: nothing holds a sentence against the content it describes, and nothing
  could. What keeps it from deciding anything is that the bytes are on the same screen, so a
  person who reads them sees what they are agreeing to whatever the sentence said. The residue is
  the same alarm fatigue as [issue #23](https://github.com/brave/bravebot/issues/23). A picture,
  which [CHECK-15](#CHECK-15) specifies and nothing builds yet, is a file to open rather than bytes
  on the screen, and [tools/vet-content.md](tools/vet-content.md#VET-4) states what that costs.

- **With auto-vetting on, the bytes are on no screen at all.** [CHECK-12](#CHECK-12) is a person
  saying in advance that a check finding nothing is enough, so on the routes it covers nobody reads
  the content and a model's word is the whole of what stood between the planner's context and a
  fetched page, or what a program printed. That is the mode working as asked rather than a flaw in
  it, and it is why the mode is off until somebody turns it on and why the settings key that turns it
  on is not readable from a checkout. What bounds it is everything the verdict does not decide: one
  slot, once, `(T,priv)`, no trust rule, and no other prompt. [labels.md](labels.md) writes out what
  an attacker who owns the content gains.

- **A screened run with nobody to ask stops where an attended one would have asked.** Failing closed
  on a check that did not complete means a rate-limited or unreachable backend keeps content back, so
  an unattended run can refuse every promotion for a reason that has nothing to do with the bytes it
  was reading, and it costs a model call per promotion on the way. What it buys is that the same run
  cannot be made to promote content by arranging for the check to fail, which is reachable by
  content the check is reading. A run that would rather have the bytes than the screening leaves the
  screening off, and gets the answers [MODE-4](permission-modes.md#MODE-4) gave before.
